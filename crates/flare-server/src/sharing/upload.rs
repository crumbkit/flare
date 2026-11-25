//! A share or a new version sent with its images.
//!
//! The upload first checks the caller's right to create or publish and
//! the recipients, which write nothing, so a caller without the right
//! never takes the decode slot. [`prepare`](prepare::prepare) then checks
//! every image before any file is put. The upload draws the share id and
//! one random file id per image, and puts the files one at a time,
//! recording each id before its put. It then commits the share or the version with the image table,
//! unless 5 minutes have passed since the upload started.
//!
//! | Outcome | The files it sent |
//! | --- | --- |
//! | The commit is written | Kept; the version lists them |
//! | A put is refused, the commit is refused, or the 5 minutes pass | Deleted before the answer |
//! | The share id is taken | Deleted, and the upload draws again, at most 3 times |
//! | The commit's outcome is unknown, or it wrote the share and a grant failed | Kept; a version may list them |
//! | The process stops part way | Kept; the sweep deletes those no version lists after 30 minutes |
//!
//! A delete that fails logs one warning with the share id and file ids,
//! and the upload still answers its first refusal; the sweep deletes what
//! is left.

mod prepare;
#[cfg(test)]
mod tests;

use prepare::Prepared;
pub(crate) use prepare::prepare;

use super::convert::{flare_error, share_error};
use super::run::{json, share_id, shared};
use super::tokens::random_id;
use super::{Answer, Refusal, Sharing, check_share, check_version};
use crate::clock::Clock;
use flare_contract::{ApiCode, Role as WireRole, ShareRequest, VersionRequest};
use flare_core::{
    CommentStore, Creators, CursorStore, FileBytes, FileId, FileStore, FileStoreError, FlareError,
    GrantStore, MediaType, NewFile, NewFlare, Principal, Recipients, ResponseStore, ShareContent,
    ShareError, ShareId, ShareStore, Teams, Timestamp, TokenStore, VersionImage, VersionImages,
};
use hyper::body::Bytes;
use std::fmt;
use std::time::{Duration, Instant};

/// How many times a create draws a new share id after `IdTaken`.
const DRAWS: usize = 3;

/// How long after its start an upload may still commit.
pub(crate) const COMMIT_WINDOW: Duration = Duration::from_secs(5 * 60);

/// One image part of an upload: its path as the Markdown wrote it, and
/// its bytes. Its `Debug` output gives only the sizes.
#[derive(Clone)]
pub(crate) struct ImagePart {
    pub(crate) path: String,
    pub(crate) bytes: Bytes,
}

impl fmt::Debug for ImagePart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "ImagePart(<{} byte path>, <{} bytes>)",
            self.path.len(),
            self.bytes.len()
        )
    }
}

/// A share or a version sent as `multipart/form-data`.
#[derive(Debug)]
pub(crate) enum Upload {
    /// `POST /api/shares` with images.
    Create(ShareRequest, Vec<ImagePart>),
    /// `POST /api/shares/{id}/versions` with images.
    Publish(String, VersionRequest, Vec<ImagePart>),
}

/// The answer of an upload, and whether a delete of a file it sent
/// failed, which the handler counts.
#[derive(Debug)]
pub(crate) struct Uploaded {
    pub(crate) answer: Answer,
    pub(crate) cleanup_failed: bool,
}

/// What a commit did with the files an upload sent.
enum Settle {
    /// Keep them: the commit wrote them into a version, or may have.
    Keep,
    /// Delete them: nothing lists them.
    Delete,
}

/// The [`Settle`] of a create that failed with `error`. A taken id is
/// handled before this.
fn settle_create(error: &FlareError) -> Settle {
    match error {
        FlareError::PartlyShared { .. } | FlareError::Share(ShareError::Unavailable) => {
            Settle::Keep
        }
        _ => Settle::Delete,
    }
}

/// The refusal when an upload's 5 minutes have passed, or a file store
/// refused a put.
fn unavailable() -> Refusal {
    ApiCode::SharesUnavailable.into()
}

/// Whether the upload that started at `started` may no longer commit.
fn late(clock: &dyn Clock, started: Instant) -> bool {
    clock.now().saturating_duration_since(started) >= COMMIT_WINDOW
}

impl<S, G, R, C, U, T, X, M, K, F> Sharing<S, G, R, C, U, T, X, M, K, F>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    C: CommentStore,
    U: CursorStore,
    T: TokenStore,
    X: Recipients,
    M: Teams,
    K: Creators,
    F: FileStore,
{
    /// Runs one upload of `who`, which started when `clock` first reads.
    pub(super) async fn uploaded(
        &self,
        who: &Principal,
        upload: Upload,
        now: Timestamp,
        clock: &dyn Clock,
    ) -> Uploaded {
        let started = clock.now();
        let mut cleanup_failed = false;
        let answer = match upload {
            Upload::Create(request, parts) => {
                self.create_upload(
                    who,
                    request,
                    parts,
                    (now, clock, started),
                    &mut cleanup_failed,
                )
                .await
            }
            Upload::Publish(id, request, parts) => {
                self.publish_upload(
                    who,
                    &id,
                    request,
                    parts,
                    (now, clock, started),
                    &mut cleanup_failed,
                )
                .await
            }
        };
        Uploaded {
            answer,
            cleanup_failed,
        }
    }

    async fn create_upload(
        &self,
        who: &Principal,
        request: ShareRequest,
        parts: Vec<ImagePart>,
        (now, clock, started): (Timestamp, &dyn Clock, Instant),
        cleanup_failed: &mut bool,
    ) -> Answer {
        let (title, text) = check_share(&request)?;
        let (with, groups) = self.named(&request.with, false).await?;
        let re = request
            .re
            .iter()
            .map(|text| share_id(text))
            .collect::<Result<Vec<_>, _>>()?;
        let flare = |id: ShareId| {
            NewFlare::new(
                id,
                title.clone(),
                ShareContent::Document(text.clone()),
                with.clone(),
                re.clone(),
            )
            .with_groups(groups.clone())
        };
        let mut id = new_share_id()?;
        self.flares
            .may_share(who, &flare(id.clone()))
            .await
            .map_err(flare_error)?;
        let images = prepare(text.as_str(), parts).await?;
        for _ in 0..DRAWS {
            let mut sent = Vec::new();
            let table = match self.send(&id, &images, &mut sent).await {
                Ok(table) if !late(clock, started) => table,
                Ok(_) => return self.refuse(&id, &sent, unavailable(), cleanup_failed).await,
                Err(refusal) => return self.refuse(&id, &sent, refusal, cleanup_failed).await,
            };
            match self
                .flares
                .share(who, flare(id.clone()).with_images(table), now)
                .await
            {
                Ok(flared) => return json(201, &shared(&flared, WireRole::Owner)),
                Err(FlareError::Share(ShareError::IdTaken)) => {
                    *cleanup_failed |= !self.clean_up(&id, &sent).await;
                }
                Err(error) => {
                    let keep = matches!(settle_create(&error), Settle::Keep);
                    let refusal = flare_error(error);
                    if keep {
                        return Err(refusal);
                    }
                    return self.refuse(&id, &sent, refusal, cleanup_failed).await;
                }
            }
            id = new_share_id()?;
        }
        Err(unavailable())
    }

    async fn publish_upload(
        &self,
        who: &Principal,
        id: &str,
        request: VersionRequest,
        parts: Vec<ImagePart>,
        (now, clock, started): (Timestamp, &dyn Clock, Instant),
        cleanup_failed: &mut bool,
    ) -> Answer {
        let id = share_id(id)?;
        let text = check_version(&request)?;
        let shares = self.flares.shares();
        shares.may_publish(who, &id).await.map_err(share_error)?;
        let images = prepare(text.as_str(), parts).await?;
        let mut sent = Vec::new();
        let table = match self.send(&id, &images, &mut sent).await {
            Ok(table) if !late(clock, started) => table,
            Ok(_) => return self.refuse(&id, &sent, unavailable(), cleanup_failed).await,
            Err(refusal) => return self.refuse(&id, &sent, refusal, cleanup_failed).await,
        };
        let content = ShareContent::Document(text);
        match shares
            .publish_with_images(who, &id, content, table, now)
            .await
        {
            Ok(number) => self.published(who, &id, number).await,
            Err(ShareError::Unavailable) => Err(share_error(ShareError::Unavailable).into()),
            Err(error) => {
                let refusal = share_error(error).into();
                self.refuse(&id, &sent, refusal, cleanup_failed).await
            }
        }
    }

    /// Puts each image under share `id`, recording each file id in `sent`
    /// before its put, and returns the image table that lists them.
    async fn send(
        &self,
        id: &ShareId,
        images: &[Prepared],
        sent: &mut Vec<FileId>,
    ) -> Result<VersionImages, Refusal> {
        let mut rows = Vec::with_capacity(images.len());
        for image in images {
            let file = FileId::from_random(random_id().ok_or_else(unavailable)?);
            let media =
                MediaType::try_from(image.format.media_type()).map_err(|_| unavailable())?;
            let bytes = FileBytes::try_from(image.bytes.clone()).map_err(|_| unavailable())?;
            let size = u32::try_from(image.bytes.len()).map_err(|_| unavailable())?;
            sent.push(file.clone());
            let new = NewFile::new(id.clone(), file.clone(), media.clone(), bytes);
            match self.files.put(new).await {
                Ok(()) => rows.push(VersionImage::new(image.path.clone(), file, media, size)),
                Err(FileStoreError::IdTaken) => {
                    // The id names another upload's file, which this one
                    // must never delete.
                    sent.pop();
                    return Err(unavailable());
                }
                Err(_) => return Err(unavailable()),
            }
        }
        VersionImages::try_from(rows).map_err(|_| unavailable())
    }

    /// Deletes the files in `sent` and answers `refusal`.
    async fn refuse(
        &self,
        id: &ShareId,
        sent: &[FileId],
        refusal: Refusal,
        cleanup_failed: &mut bool,
    ) -> Answer {
        *cleanup_failed |= !self.clean_up(id, sent).await;
        Err(refusal)
    }

    /// Deletes each file in `sent` under share `id`, and whether every
    /// delete succeeded. A failure logs one warning that names the ids.
    async fn clean_up(&self, id: &ShareId, sent: &[FileId]) -> bool {
        let mut failed = Vec::new();
        for file in sent {
            if self.files.delete(id, file).await.is_err() {
                failed.push(file.as_str().to_owned());
            }
        }
        if failed.is_empty() {
            return true;
        }
        tracing::warn!(
            share = id.as_str(),
            files = ?failed,
            "files that no version lists could not be deleted; the sweep deletes them after 30 minutes"
        );
        false
    }
}

/// A new random share id.
fn new_share_id() -> Result<ShareId, Refusal> {
    random_id()
        .map(ShareId::from_random)
        .ok_or_else(unavailable)
}

//! A share or a version whose content is one uploaded HTML file.
//!
//! The file is stored as one file of the share, and the version names it.
//! The outcomes follow the table in `upload.rs`: the file is put before the
//! version is written and is deleted when the write is refused, so a
//! version never names a file that is missing.

use super::{DRAWS, Settle, Sharing, late, new_share_id, settle_create, unavailable};
use crate::clock::Clock;
use crate::sharing::convert::{flare_error, share_error};
use crate::sharing::run::{json, share_id, shared};
use crate::sharing::tokens::random_id;
use crate::sharing::{Answer, Refusal, check_html_share, check_html_version};
use flare_contract::{Role as WireRole, ShareRequest, VersionRequest};
use flare_core::{
    CommentStore, Creators, CursorStore, FileBytes, FileId, FileStore, FileStoreError, FlareError,
    GrantStore, HtmlFile, MediaType, NewFile, NewFlare, Principal, Recipients, ResponseStore,
    ShareContent, ShareError, ShareId, ShareStore, Teams, Timestamp, TokenStore,
};
use hyper::body::Bytes;
use std::time::Instant;

/// The media type that the file is stored under.
const HTML: &str = "text/html";

/// When the upload started, and the clocks it is judged by.
type Timing<'a> = (Timestamp, &'a dyn Clock, Instant);

/// The content of a version that names `file`, which holds `size` bytes.
fn content(file: &FileId, size: usize) -> Result<ShareContent, Refusal> {
    HtmlFile::new(file.clone(), size)
        .map(ShareContent::Html)
        .map_err(|_| unavailable())
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
    /// Puts `bytes` as file `file` of share `id`, and records the file in
    /// `sent` before the put.
    async fn put_html(
        &self,
        id: &ShareId,
        file: &FileId,
        bytes: &Bytes,
        sent: &mut Vec<FileId>,
    ) -> Result<(), Refusal> {
        let media = MediaType::try_from(HTML).map_err(|_| unavailable())?;
        let body = FileBytes::try_from(bytes.to_vec()).map_err(|_| unavailable())?;
        sent.push(file.clone());
        match self
            .files
            .put(NewFile::new(id.clone(), file.clone(), media, body))
            .await
        {
            Ok(()) => Ok(()),
            Err(FileStoreError::IdTaken) => {
                // The id names another upload's file, which this one must
                // never delete.
                sent.pop();
                Err(unavailable())
            }
            Err(_) => Err(unavailable()),
        }
    }

    pub(super) async fn create_html(
        &self,
        who: &Principal,
        request: ShareRequest,
        bytes: Bytes,
        (now, clock, started): Timing<'_>,
        cleanup_failed: &mut bool,
    ) -> Answer {
        let title = check_html_share(&request, &bytes)?;
        let (with, groups) = self.named(&request.with, false).await?;
        let re = request
            .re
            .iter()
            .map(|text| share_id(text))
            .collect::<Result<Vec<_>, _>>()?;
        let file = FileId::from_random(random_id().ok_or_else(unavailable)?);
        let content = content(&file, bytes.len())?;
        let flare = |id: ShareId| {
            NewFlare::new(id, title.clone(), content.clone(), with.clone(), re.clone())
                .with_groups(groups.clone())
        };
        let mut id = new_share_id()?;
        self.flares
            .may_share(who, &flare(id.clone()))
            .await
            .map_err(flare_error)?;
        for _ in 0..DRAWS {
            let mut sent = Vec::new();
            match self.put_html(&id, &file, &bytes, &mut sent).await {
                Ok(()) if !late(clock, started) => {}
                Ok(()) => return self.refuse(&id, &sent, unavailable(), cleanup_failed).await,
                Err(refusal) => return self.refuse(&id, &sent, refusal, cleanup_failed).await,
            }
            match self.flares.share(who, flare(id.clone()), now).await {
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

    pub(super) async fn publish_html(
        &self,
        who: &Principal,
        id: &str,
        request: VersionRequest,
        bytes: Bytes,
        (now, clock, started): Timing<'_>,
        cleanup_failed: &mut bool,
    ) -> Answer {
        let id = share_id(id)?;
        check_html_version(&request, &bytes)?;
        let shares = self.flares.shares();
        shares.may_publish(who, &id).await.map_err(share_error)?;
        let file = FileId::from_random(random_id().ok_or_else(unavailable)?);
        let content = content(&file, bytes.len())?;
        let mut sent = Vec::new();
        match self.put_html(&id, &file, &bytes, &mut sent).await {
            Ok(()) if !late(clock, started) => {}
            Ok(()) => return self.refuse(&id, &sent, unavailable(), cleanup_failed).await,
            Err(refusal) => return self.refuse(&id, &sent, refusal, cleanup_failed).await,
        }
        match shares.publish(who, &id, content, now).await {
            Ok(number) => self.published(who, &id, number).await,
            Err(ShareError::Unavailable) => Err(share_error(ShareError::Unavailable).into()),
            Err(error) => {
                let refusal = share_error(error).into();
                self.refuse(&id, &sent, refusal, cleanup_failed).await
            }
        }
    }
}

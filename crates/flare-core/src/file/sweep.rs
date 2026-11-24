//! [`sweep_files`]: deletes the stored files that no version lists once
//! they are old enough that no create or publish can still list them.
//!
//! A create or publish stores its files, then writes the version that
//! lists them, and ends its writes within 30 minutes of its first put. A
//! file stored at least [`SWEEP_AGE`] ago that no version of its share
//! lists, or whose share does not exist, was left by a request that failed
//! or crashed, and the sweep deletes it. A younger file is never deleted,
//! and neither is a file any version lists.

use crate::file::FileId;
use crate::share::{ShareId, ShareVersion};
use crate::store::{
    FileCursor, FileStore, FileStoreError, MAX_FILE_PAGE, ShareStore, ShareStoreError,
};
use crate::time::Timestamp;
use std::collections::{BTreeMap, BTreeSet};

/// How old an unlisted file must be before the sweep deletes it, in
/// seconds: 30 minutes.
pub const SWEEP_AGE: u64 = 30 * 60;

/// The most files one sweep examines.
pub const MAX_SWEPT_FILES: usize = 10_000;

/// What one sweep did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SweepReport {
    /// The files it read from the store's list.
    pub examined: usize,
    /// The unlisted files it deleted.
    pub deleted: usize,
    /// The files it left because a store did not answer, which the next
    /// sweep examines again.
    pub undecided: usize,
    /// Whether it stopped at its limit before the end of the list.
    pub stopped_early: bool,
}

/// Deletes each file stored at least [`SWEEP_AGE`] before `now` that no
/// version of its share lists, or whose share does not exist, examining at
/// most `limit` files.
///
/// # Errors
///
/// Returns [`FileStoreError::Unavailable`] when the file store cannot list.
pub async fn sweep_files<S: ShareStore, F: FileStore>(
    shares: &S,
    files: &F,
    now: Timestamp,
    limit: usize,
) -> Result<SweepReport, FileStoreError> {
    let cutoff = now.as_secs().saturating_sub(SWEEP_AGE);
    let mut report = SweepReport::default();
    let mut listed: BTreeMap<ShareId, Listed> = BTreeMap::new();
    let mut after: Option<FileCursor> = None;
    if limit == 0 {
        report.stopped_early = true;
        return Ok(report);
    }
    loop {
        let room = limit.saturating_sub(report.examined);
        let page = files
            .list(after.as_ref(), room.clamp(1, MAX_FILE_PAGE))
            .await?;
        let more_in_page = page.files.len() > room;
        for file in page.files.iter().take(room) {
            report.examined += 1;
            if file.stored().as_secs() > cutoff {
                continue;
            }
            if !listed.contains_key(file.share()) {
                let read = listed_files(shares, file.share()).await;
                listed.insert(file.share().clone(), read);
            }
            match listed.get(file.share()) {
                Some(Listed::Files(ids)) if ids.contains(file.id()) => {}
                Some(Listed::Files(_) | Listed::NoShare) => {
                    match files.delete(file.share(), file.id()).await {
                        Ok(()) => report.deleted += 1,
                        Err(_) => report.undecided += 1,
                    }
                }
                Some(Listed::Unknown) | None => report.undecided += 1,
            }
        }
        if report.examined >= limit && (more_in_page || page.next.is_some()) {
            report.stopped_early = true;
            return Ok(report);
        }
        match page.next {
            Some(next) => after = Some(next),
            None => return Ok(report),
        }
    }
}

/// What the share store says a share lists.
enum Listed {
    /// The share does not exist.
    NoShare,
    /// The files its versions list together.
    Files(BTreeSet<FileId>),
    /// The share store did not answer.
    Unknown,
}

/// Every file that any version of `share` lists.
async fn listed_files<S: ShareStore>(shares: &S, share: &ShareId) -> Listed {
    let latest = match shares.get(share).await {
        Ok(found) => found.latest(),
        Err(ShareStoreError::NotFound) => return Listed::NoShare,
        Err(_) => return Listed::Unknown,
    };
    let mut ids = BTreeSet::new();
    for number in 1..=latest.get() {
        let Ok(number) = ShareVersion::try_from(number) else {
            return Listed::Unknown;
        };
        match shares.version(share, number).await {
            Ok(version) => {
                ids.extend(version.images().rows().iter().map(|row| row.file().clone()));
            }
            Err(_) => return Listed::Unknown,
        }
    }
    Listed::Files(ids)
}

#[cfg(test)]
mod tests;

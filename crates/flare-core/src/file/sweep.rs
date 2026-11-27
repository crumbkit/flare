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
use crate::share::{ShareContent, ShareId, ShareVersion};
use crate::store::{
    FileCursor, FileStore, FileStoreError, ListedFile, MAX_FILE_PAGE, ShareStore, ShareStoreError,
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
/// The walk starts at the first file of `start`, or of the next share
/// after it, runs to the end of the list, and then wraps to the start of
/// the list and stops at the first file it already examined. A caller that
/// passes a random share id each run reaches every part of a list longer
/// than `limit` over enough runs. A store whose
/// [`cursor_at`](FileStore::cursor_at) gives no cursor is walked once from
/// its start.
///
/// # Errors
///
/// Returns [`FileStoreError::Unavailable`] when the file store cannot list.
pub async fn sweep_files<S: ShareStore, F: FileStore>(
    shares: &S,
    files: &F,
    now: Timestamp,
    limit: usize,
    start: &ShareId,
) -> Result<SweepReport, FileStoreError> {
    let mut sweep = Sweep {
        shares,
        files,
        cutoff: now.as_secs().saturating_sub(SWEEP_AGE),
        limit,
        report: SweepReport::default(),
        listed: BTreeMap::new(),
        seen: BTreeSet::new(),
    };
    let from = files.cursor_at(start);
    let wraps = from.is_some();
    let mut ended = sweep.walk(from, Pass::First).await?;
    if wraps && matches!(ended, Ended::AtEnd) {
        ended = sweep.walk(None, Pass::Wrapped).await?;
    }
    sweep.report.stopped_early = matches!(ended, Ended::AtLimit);
    Ok(sweep.report)
}

/// The key of one listed file.
type Key = (ShareId, FileId);

/// Which part of the walk runs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// From the start share to the end of the list.
    First,
    /// From the start of the list to the first file the first pass named.
    Wrapped,
}

/// Why a part of the walk ended.
enum Ended {
    /// The list ended, or the wrapped pass reached a file already examined.
    AtEnd,
    /// The sweep examined its limit and a file was left.
    AtLimit,
}

/// One sweep's stores, limits and progress.
struct Sweep<'a, S, F> {
    shares: &'a S,
    files: &'a F,
    cutoff: u64,
    limit: usize,
    report: SweepReport,
    listed: BTreeMap<ShareId, Listed>,
    /// The files the first pass examined, where the wrapped pass stops.
    seen: BTreeSet<Key>,
}

impl<S: ShareStore, F: FileStore> Sweep<'_, S, F> {
    /// Walks the list from `after` until it ends, the limit is reached or,
    /// on the wrapped pass, a file the first pass examined comes up.
    async fn walk(
        &mut self,
        mut after: Option<FileCursor>,
        pass: Pass,
    ) -> Result<Ended, FileStoreError> {
        loop {
            let room = self.limit.saturating_sub(self.report.examined);
            let page = self
                .files
                .list(after.as_ref(), room.clamp(1, MAX_FILE_PAGE))
                .await?;
            for file in &page.files {
                let key = (file.share().clone(), file.id().clone());
                if pass == Pass::Wrapped && self.seen.contains(&key) {
                    return Ok(Ended::AtEnd);
                }
                if self.report.examined >= self.limit {
                    return Ok(Ended::AtLimit);
                }
                if pass == Pass::First {
                    self.seen.insert(key);
                }
                self.examine(file).await;
            }
            match page.next {
                Some(next) => after = Some(next),
                None => return Ok(Ended::AtEnd),
            }
        }
    }

    /// Examines one file, and deletes it when it is old enough and no
    /// version of its share lists it.
    async fn examine(&mut self, file: &ListedFile) {
        self.report.examined += 1;
        if file.stored().as_secs() > self.cutoff {
            return;
        }
        if !self.listed.contains_key(file.share()) {
            let read = listed_files(self.shares, file.share()).await;
            self.listed.insert(file.share().clone(), read);
        }
        match self.listed.get(file.share()) {
            Some(Listed::Files(ids)) if ids.contains(file.id()) => {}
            Some(Listed::Files(_) | Listed::NoShare) => {
                match self.files.delete(file.share(), file.id()).await {
                    Ok(()) => self.report.deleted += 1,
                    Err(_) => self.report.undecided += 1,
                }
            }
            Some(Listed::Unknown) | None => self.report.undecided += 1,
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
                if let ShareContent::Html(html) = version.content() {
                    ids.insert(html.file().clone());
                }
            }
            Err(_) => return Listed::Unknown,
        }
    }
    Listed::Files(ids)
}

#[cfg(test)]
mod tests;

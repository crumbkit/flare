//! A page's history: the walks of its record's log from `HEAD`, and the
//! check that a version is one of them.
//!
//! Every walk here starts at `HEAD`, because a version is only a commit
//! that changed the page's record and that `HEAD` reaches. `board log --at
//! <commit>` starts at any commit the repository holds, including one on
//! another branch, so a walk from the version alone cannot decide it.

use super::{at, record_path, unavailable};
use crate::process::{Board, Rows};
use crate::report::{Failure, LogRow};
use flare_core::{PageName, PagePin, PageStoreError, PageVersion, PublishedPage};
use std::sync::Arc;

/// The commits that changed the page's record, newest first. A walk that
/// stopped after 100,000 commits or reached a shallow clone's boundary
/// answers `Unavailable`, because its list may miss versions or name the
/// boundary as one.
pub(crate) async fn versions(
    board: &Arc<Board>,
    name: &PageName,
) -> Result<Vec<PageVersion>, PageStoreError> {
    match board.log(&record_path(name), None, Rows::All).await {
        Ok(log) if log.truncated || !log.summary.complete => Err(PageStoreError::Unavailable),
        Ok(log) => {
            let mut versions = Vec::with_capacity(log.data.len());
            for row in log.data {
                versions.push(
                    PageVersion::try_from(row.commit.as_str())
                        .map_err(|_| PageStoreError::Unavailable)?,
                );
            }
            Ok(versions)
        }
        Err(Failure::Board(failure)) if failure.code == "board::path::not_found" => {
            Err(PageStoreError::NotFound)
        }
        Err(other) => unavailable(other),
    }
}

/// The newest row of the page record's log from `HEAD`, which a version
/// needs a complete walk to trust. No row from a complete walk is
/// `NotFound`. A walk that stopped after 100,000 commits or reached a
/// shallow clone's boundary is `Unavailable`, because the row it lists, or
/// the row it missed, may not be the page's last change.
pub(crate) async fn newest(board: &Arc<Board>, name: &PageName) -> Result<LogRow, PageStoreError> {
    match board.log(&record_path(name), None, Rows::Newest).await {
        Ok(log) if !log.summary.complete => Err(PageStoreError::Unavailable),
        Ok(log) => log.data.into_iter().next().ok_or(PageStoreError::NotFound),
        Err(Failure::Board(failure)) if failure.code == "board::path::not_found" => {
            Err(PageStoreError::NotFound)
        }
        Err(other) => unavailable(other),
    }
}

/// What the newest row from `HEAD` says about a version.
enum Newest {
    /// The version is the newest row of a complete walk, so it changed the
    /// record, `HEAD` reaches it and it is no shallow boundary.
    Is,
    /// A complete walk lists another newest row, or no row, so the walk of
    /// every row from `HEAD` decides whether it lists the version.
    Other,
    /// The walk was not complete, past 100,000 commits or at a shallow
    /// clone's boundary. A walk of every row from `HEAD` would stop at the
    /// same place, so it would decide nothing.
    Unreached,
}

/// Whether `version` is the newest row of the page record's log from
/// `HEAD`. A failed read answers `Unavailable`.
async fn is_newest(
    board: &Arc<Board>,
    name: &PageName,
    version: &str,
) -> Result<Newest, PageStoreError> {
    match board.log(&record_path(name), None, Rows::Newest).await {
        Ok(log) if !log.summary.complete => Ok(Newest::Unreached),
        Ok(log) => match log.data.first() {
            Some(row) if row.commit == version && row.blob.is_some() => Ok(Newest::Is),
            _ => Ok(Newest::Other),
        },
        Err(Failure::Board(failure)) if failure.code == "board::path::not_found" => {
            Ok(Newest::Other)
        }
        Err(other) => unavailable(other),
    }
}

/// Answers `Ok` when the walk of the page record's log from `HEAD` lists
/// `version`, and `NotFound` when a complete walk does not. A failed read
/// answers `Unavailable`.
///
/// This does less than a reachability query would: board has none, so the
/// check is the record's log from `HEAD`. When that walk is not complete,
/// past 100,000 commits or at a shallow clone's boundary, and does not list
/// the version, the version is accepted unchecked, so a page whose history
/// is longer than the walk can still be pinned (an accepted gap in
/// docs/board-store.md).
async fn reachable(
    board: &Arc<Board>,
    name: &PageName,
    version: &str,
) -> Result<(), PageStoreError> {
    let log = match board.log(&record_path(name), None, Rows::All).await {
        Ok(log) => log,
        Err(Failure::Board(failure)) if failure.code == "board::path::not_found" => {
            return Err(PageStoreError::NotFound);
        }
        Err(other) => return unavailable(other),
    };
    let listed = log.data.iter().any(|row| row.commit == version);
    let decided = log.summary.complete && !log.truncated;
    match (listed, decided) {
        (true, _) => Ok(()),
        // Accepted gap: the walk stopped, past 100,000 commits or at a
        // shallow clone's boundary, before it listed the version, so the
        // version is accepted unchecked. A reachability query in board
        // would replace this walk and decide the version.
        (false, false) => Ok(()),
        (false, true) => Err(PageStoreError::NotFound),
    }
}

/// The page at `pin`, for a pin about to be written: `at`'s check, and
/// then that `HEAD` reaches the version, so a binding or an origin never
/// pins a version that `versions` does not list (invariant 4). A version
/// on another branch, or one a reset orphaned, is not found.
///
/// When the version is the newest row of a complete walk from `HEAD`, that
/// read decides it. Otherwise the walk of every row from `HEAD` does. When
/// the walk from `HEAD` is not complete and does not list the version, the
/// version is accepted unchecked (an accepted gap in docs/board-store.md).
pub(crate) async fn pinned(
    board: &Arc<Board>,
    pin: &PagePin,
) -> Result<PublishedPage, PageStoreError> {
    let page = at(board, pin).await?;
    let version = pin.version().as_str();
    match is_newest(board, pin.name(), version).await? {
        Newest::Is => {}
        Newest::Other => reachable(board, pin.name(), version).await?,
        // Accepted gap: the walk stopped, past 100,000 commits or at a
        // shallow clone's boundary, so the version is accepted unchecked. A
        // reachability query in board would replace this walk and decide
        // the version.
        Newest::Unreached => {}
    }
    Ok(page)
}

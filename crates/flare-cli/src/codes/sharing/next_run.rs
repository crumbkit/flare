//! What a missing, unreadable or failed answer means for running a sharing
//! command again, and the recoveries that follow from it.

use super::SharingFailed;
use crate::codes::Failed;
use crumbkit::{Recovery, Then};
use flare_contract::ApiCode;

/// What a missing or unreadable answer means for running the command
/// again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextRun {
    /// The request did nothing on the server: the server refused it, or
    /// flare could not connect and sent nothing. The next run starts clean.
    Clean,
    /// The server may have done what the request asked: it answered with a
    /// success status that flare cannot read, or it did not answer after
    /// the request was sent. A command whose next run ends in the same
    /// state, such as a read or `flare grant`, keeps this, and a command
    /// that writes a share or a version turns it into
    /// [`NextRun::CheckShares`] with `Failed::after_a_write`.
    MaybeDone,
    /// The server may have made the share or the version, and the share API
    /// takes no idempotency key, so a second run could make a second one.
    CheckShares,
}

/// The recovery after a write that may have happened: check `flare
/// shares` before running the command again.
pub(super) fn check_shares() -> Recovery {
    Recovery::after(
        "check that flare shares does not already list the share or the version this command meant to make",
        Then::Retry,
    )
}

/// The recovery of [`SharingFailed::BadAnswer`]: check the server and run
/// the command again, or, after a write that may have happened, check
/// `flare shares` first.
pub(super) fn bad_answer(next: NextRun) -> Recovery {
    match next {
        NextRun::Clean | NextRun::MaybeDone => Recovery::after(
            "check that the server runs flare with the share API",
            Then::Retry,
        ),
        NextRun::CheckShares => check_shares(),
    }
}

/// The recovery of [`SharingFailed::Unreachable`] and
/// [`SharingFailed::ServerError`]: run the command again, or, after a write
/// that may have happened, check `flare shares` first.
pub(super) fn retry(next: NextRun) -> Recovery {
    match next {
        NextRun::Clean | NextRun::MaybeDone => Recovery::Retry,
        NextRun::CheckShares => check_shares(),
    }
}

impl Failed {
    /// The same failure for a command that writes a share or a version. A
    /// request that may have reached the server, whose answer flare could
    /// not read or did not get, that the server failed or stopped while it
    /// ran, or that the server refused with `shares::unavailable` because
    /// it does not know the write's outcome, may have made the share or the
    /// version, so its recovery checks `flare shares` before the command
    /// runs again.
    pub(crate) fn after_a_write(mut self) -> Self {
        match &mut self {
            Self::Sharing(
                SharingFailed::BadAnswer { next, .. }
                | SharingFailed::Unreachable { next, .. }
                | SharingFailed::ServerError { next, .. },
            ) if *next == NextRun::MaybeDone => *next = NextRun::CheckShares,
            Self::Refused(refused) if refused.code == ApiCode::SharesUnavailable => {
                refused.maybe_written = true;
            }
            _ => {}
        }
        self
    }
}

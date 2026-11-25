//! `flare files sweep`: deletes the image files that a failed or
//! interrupted create or publish left in the hosted site's bucket, with
//! flare-core's [`sweep_files`].
//!
//! It reads the table and the bucket from `hosted.stores` in the same
//! config file as `flare serve`, and checks them as `flare serve` does
//! before it lists a file. One run examines at most [`MAX_SWEPT_FILES`]
//! files and deletes each one stored at least 30 minutes ago that no
//! version of its share lists, or whose share does not exist. Each run
//! starts at a random share and wraps to the start of the list, so runs
//! over a bucket of more than 10,000 files reach every part of it. The
//! deployment runs it on a schedule; `flare serve` runs no sweep.

use crate::cli::SweepArgs;
use crate::codes::Failed;
use crate::config::{self, Site};
use crate::env::Snapshot;
use crumbkit::{Document, Row};
use flare_aws::Stores;
use flare_core::{MAX_SWEPT_FILES, ShareId, Timestamp, sweep_files};
use schemars::JsonSchema;
use serde::Serialize;
use std::hash::{BuildHasher as _, RandomState};
use std::time::{SystemTime, UNIX_EPOCH};

/// What `flare files sweep` did.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct Swept {
    /// The files it read from the bucket's list.
    #[row(column)]
    pub(crate) examined: usize,
    /// The files it deleted, which no version listed.
    #[row(column)]
    pub(crate) deleted: usize,
    /// The files it left because the table or the bucket did not answer;
    /// the next run examines them again.
    #[row(column)]
    pub(crate) undecided: usize,
    /// Whether it stopped at 10,000 files before it examined every file; the
    /// next run starts at another random share.
    #[row(column)]
    pub(crate) stopped_early: bool,
}

/// The document of `flare files sweep`.
pub(crate) const FILES_SWEEP: Document<Swept> = Document::new(
    "flare",
    "files-sweep",
    1,
    "How many files a sweep examined and deleted, and whether it stopped early.",
);

/// Runs `flare files sweep` on the stores that the config names.
///
/// # Errors
///
/// Returns [`Failed::ConfigInvalid`] when the config names no
/// `hosted.stores`, and [`Failed::StoresUnavailable`] when the table or
/// the bucket is missing or cannot be read.
pub(crate) async fn sweep(args: &SweepArgs, env: &Snapshot) -> Result<Swept, Failed> {
    let loaded = config::load(args.config.as_deref(), env)?;
    let stores = match loaded.site {
        Site::Hosted(hosted) => hosted.sharing.map(|sharing| sharing.stores),
        Site::Local { .. } => None,
    };
    let Some(stores) = stores else {
        return Err(Failed::ConfigInvalid {
            path: config::path(args.config.as_deref(), env).map_or_else(
                || "the config".to_owned(),
                |path| path.display().to_string(),
            ),
            problem: "it names no hosted.stores, so flare files sweep has no bucket to sweep"
                .to_owned(),
        });
    };
    let stores = Stores::connect(&stores).await;
    let unavailable = |problem: String| Failed::StoresUnavailable { problem };
    stores
        .check()
        .await
        .map_err(|error| unavailable(error.to_string()))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let report = sweep_files(
        &stores.shares,
        &stores.files,
        Timestamp::from_secs(now),
        MAX_SWEPT_FILES,
        &random_share(),
    )
    .await
    .map_err(|_| unavailable("the S3 bucket's files could not be listed".to_owned()))?;
    Ok(Swept {
        examined: report.examined,
        deleted: report.deleted,
        undecided: report.undecided,
        stopped_early: report.stopped_early,
    })
}

/// A share id to start the sweep at, different on each run.
///
/// Shortcut: it takes its bits from the standard library's `RandomState`,
/// whose keys the operating system seeds once per process, so it needs no
/// random-number crate. The start point only spreads runs across the list
/// and guards nothing, so it needs no cryptographic quality.
fn random_share() -> ShareId {
    let [first, second] = [1_u8, 2].map(|round| RandomState::new().hash_one(round));
    let mut bytes = [0_u8; 16];
    let (high, low) = bytes.split_at_mut(8);
    high.copy_from_slice(&first.to_be_bytes());
    low.copy_from_slice(&second.to_be_bytes());
    ShareId::from_random(bytes)
}

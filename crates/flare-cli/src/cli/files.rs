//! The arguments and help of `flare files`.

use crate::files::FILES_SWEEP;
use clap::{Args, Subcommand};
use crumbkit::Yes;
use crumbkit::help::{Effects, Example, Leaf};
use std::path::PathBuf;

/// The commands of `flare files`.
#[derive(Debug, Subcommand)]
pub(crate) enum FilesVerb {
    /// Delete the image files that no version lists from the hosted site's bucket
    ///
    /// A failed or interrupted share can leave image files in the bucket
    /// that no version lists, and no reader can open them. The sweep
    /// deletes each such file stored at least 30 minutes ago, and keeps
    /// every file a version lists. One run examines at most 10,000 files.
    /// Run it on a schedule, such as hourly; flare serve runs no sweep.
    Sweep(SweepArgs),
}

/// The arguments of `flare files sweep`.
#[derive(Debug, Args)]
pub(crate) struct SweepArgs {
    /// The config file whose hosted.stores names the table and the bucket; without it, $XDG_CONFIG_HOME/flare/config.json or ~/.config/flare/config.json
    #[arg(long, value_name = "PATH")]
    pub(crate) config: Option<PathBuf>,
    /// The confirmation that `--yes` gives without a question.
    #[command(flatten)]
    pub(crate) yes: Yes,
}

/// The help of `flare files sweep`.
pub(super) const SWEEP_LEAF: Leaf = Leaf::new(&["files", "sweep"])
    .examples(&[Example::new(
        &["--config", "/etc/flare/config.json", "--yes"],
        "Delete the files that no version lists from the bucket that /etc/flare/config.json names",
    )
    .needs(&["stores"])])
    .effects(Effects::destructive().idempotent())
    .document(FILES_SWEEP);

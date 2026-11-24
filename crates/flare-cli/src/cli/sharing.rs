//! The arguments and help of `flare login`, `logout`, `share`, `show`,
//! `grant`, `shares` and `activity`.

use clap::Args;
use crumbkit::{DryRun, Page, Yes};
use std::num::NonZeroU32;
use std::path::PathBuf;

/// The most flares one `flare activity` names.
const MAX_FLARES: usize = 16;

/// The arguments of `flare login`.
#[derive(Debug, Args)]
pub(crate) struct LoginArgs {
    /// The flare server, such as https://flare.example.org
    #[arg(value_name = "SERVER")]
    pub(crate) server: String,
    /// The token's label; without it, the label of this computer's earlier login to the server, else a new flare-cli-<hex>
    #[arg(long, value_name = "LABEL")]
    pub(crate) label: Option<String>,
}

/// The arguments of `flare logout`.
#[derive(Debug, Args)]
pub(crate) struct LogoutArgs {
    /// The confirmation that `--yes` gives without a question.
    #[command(flatten)]
    pub(crate) yes: Yes,
}

/// The arguments of `flare share`.
#[derive(Debug, Args)]
pub(crate) struct ShareArgs {
    /// The file to share: .md for Markdown
    #[arg(value_name = "FILE")]
    pub(crate) file: PathBuf,
    /// The title; without it, the file name without its ending
    #[arg(long, value_name = "TITLE")]
    pub(crate) title: Option<String>,
    /// The addresses to grant Reader, repeated or comma-separated
    #[arg(long, value_name = "ADDRESS", value_delimiter = ',')]
    pub(crate) with: Vec<String>,
    /// The flares it responds to, whose owners become Readers; repeated or comma-separated
    #[arg(long, value_name = "ID", value_delimiter = ',')]
    pub(crate) re: Vec<String>,
    /// Publish the file as your share's next version
    ///
    /// It changes only the content; grants stay with flare grant.
    #[arg(long, value_name = "ID", conflicts_with_all = ["title", "with", "re"])]
    pub(crate) update: Option<String>,
    #[command(flatten)]
    pub(crate) dry_run: DryRun,
    /// With --dry-run, write the HTML here
    #[arg(long, value_name = "PATH", requires = "crumbkit_dry_run")]
    pub(crate) out: Option<PathBuf>,
}

/// The arguments of `flare show`.
#[derive(Debug, Args)]
pub(crate) struct ShowArgs {
    /// The share to show
    #[arg(value_name = "ID")]
    pub(crate) id: String,
    /// The version to show, a number from 1; without it, the latest
    #[arg(long, value_name = "N")]
    pub(crate) version: Option<NonZeroU32>,
}

/// The arguments of `flare grant`.
#[derive(Debug, Args)]
pub(crate) struct GrantArgs {
    /// The share to grant
    #[arg(value_name = "ID")]
    pub(crate) id: String,
    /// The addresses to grant Reader, repeated or separated by commas
    #[arg(long, value_name = "ADDRESS", value_delimiter = ',')]
    pub(crate) with: Vec<String>,
    /// The flares the share responds to, repeated or separated by commas; only its owner may add them, and the flares' owners become Readers of it
    #[arg(long, value_name = "ID", value_delimiter = ',')]
    pub(crate) re: Vec<String>,
}

/// The arguments of `flare shares`.
#[derive(Debug, Args)]
pub(crate) struct SharesArgs {
    /// List the shares granted to you in place of your own
    #[arg(long)]
    pub(crate) with_me: bool,
    /// List the shares after this cursor, which an earlier flare shares printed
    #[arg(long, value_name = "CURSOR")]
    pub(crate) cursor: Option<String>,
    /// How many shares to list; --all follows the cursor to the end.
    #[command(flatten)]
    pub(crate) page: Page<50, 500>,
}

/// The arguments of `flare activity`.
#[derive(Debug, Args)]
pub(crate) struct ActivityArgs {
    /// The flares whose responses and comments to list, at most 16
    #[arg(value_name = "FLARE", required = true, num_args = 1..=MAX_FLARES)]
    pub(crate) flares: Vec<String>,
    /// List only activity after this cursor, which an earlier call printed
    #[arg(long, value_name = "CURSOR")]
    pub(crate) since: Option<String>,
    /// Continue from the cursor saved as NAME, the larger of this computer's and the server's, and save where this call ends
    #[arg(long, value_name = "NAME")]
    pub(crate) since_last: Option<String>,
    /// How many items to list; --all follows the cursor to the newest.
    #[command(flatten)]
    pub(crate) page: Page<50, 100>,
}

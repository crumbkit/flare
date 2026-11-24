//! The arguments and help of `flare login`, `logout`, `share`, `show`,
//! `grant`, `revoke`, `grants`, `shares` and `activity`.

use crate::documents::{GRANT, GRANTS, REVOKE};
use clap::Args;
use crumbkit::help::{Effects, Example, Leaf};
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
    /// Readers, comma-separated: addresses, team:<name>, everyone
    #[arg(long, value_name = "WHO", value_delimiter = ',')]
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
    /// Who to grant Reader, repeated or separated by commas: an address, team:<name> for a team you belong to, or everyone
    #[arg(long, value_name = "WHO", value_delimiter = ',')]
    pub(crate) with: Vec<String>,
    /// Grant Reader to everyone signed in whom the server's sign-in rule covers; only the share's owner may
    #[arg(long)]
    pub(crate) everyone: bool,
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
    /// With --with-me, also list the shares open to everyone signed in
    #[arg(long, requires = "with_me")]
    pub(crate) everyone: bool,
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
    /// The flares whose responses to list, at most 16
    #[arg(value_name = "FLARE", required = true, num_args = 1..=MAX_FLARES)]
    pub(crate) flares: Vec<String>,
    /// List only responses after this cursor, which an earlier call printed
    #[arg(long, value_name = "CURSOR")]
    pub(crate) since: Option<String>,
    /// How many responses to list; --all follows the cursor to the newest.
    #[command(flatten)]
    pub(crate) page: Page<50, 100>,
}

/// The arguments of `flare revoke`.
#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("who").required(true).multiple(true).args(["with", "everyone"])))]
pub(crate) struct RevokeArgs {
    /// The share whose grants end
    #[arg(value_name = "ID")]
    pub(crate) id: String,
    /// Whose grant ends, repeated or separated by commas: an address, team:<name> or everyone
    #[arg(long, value_name = "WHO", value_delimiter = ',')]
    pub(crate) with: Vec<String>,
    /// End the grant to everyone signed in; only the share's owner may
    #[arg(long)]
    pub(crate) everyone: bool,
    /// The confirmation that `--yes` gives without a question.
    #[command(flatten)]
    pub(crate) yes: Yes,
}

/// The arguments of `flare grants`.
#[derive(Debug, Args)]
pub(crate) struct GrantsArgs {
    /// The share whose grants to list; you must own it or hold Admin on it
    #[arg(value_name = "ID")]
    pub(crate) id: String,
}

/// The help of `flare grant`.
pub(super) const GRANT_LEAF: Leaf = Leaf::new(&["grant"])
    .examples(&[
        Example::new(
            &[
                "3f9c0a17e2b44d6f8a1c5b7e9d20f4a6",
                "--with",
                "mei@example.org",
            ],
            "Grant a share to one more person",
        )
        .needs(&["server"]),
        Example::new(
            &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6", "--with", "team:design"],
            "Grant a share to the members of a team you belong to",
        )
        .needs(&["server"]),
        Example::new(
            &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6", "--everyone"],
            "Open a share you own to everyone signed in",
        )
        .needs(&["server"]),
    ])
    .effects(Effects::additive())
    .document(GRANT);

/// The help of `flare revoke`.
pub(super) const REVOKE_LEAF: Leaf = Leaf::new(&["revoke"])
    .examples(&[
        Example::new(
            &[
                "3f9c0a17e2b44d6f8a1c5b7e9d20f4a6",
                "--with",
                "mei@example.org",
                "--yes",
            ],
            "End one person's grant on a share",
        )
        .needs(&["server"]),
        Example::new(
            &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6", "--everyone", "--yes"],
            "Close a share you own to everyone signed in",
        )
        .needs(&["server"]),
    ])
    .effects(Effects::destructive())
    .document(REVOKE);

/// The help of `flare grants`.
pub(super) const GRANTS_LEAF: Leaf = Leaf::new(&["grants"])
    .examples(&[Example::new(
        &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6"],
        "List the people, teams and everyone grants of a share",
    )
    .needs(&["server"])])
    .effects(Effects::read_only())
    .document(GRANTS);

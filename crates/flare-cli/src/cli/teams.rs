//! The arguments and help of `flare teams`.

use clap::{Args, Subcommand};
use crumbkit::{Page, Yes};

/// The most teams `flare teams show` shows at once.
pub(crate) const MAX_SHOWN: usize = 100;

/// The commands of `flare teams`.
#[derive(Debug, Subcommand)]
pub(crate) enum TeamsVerb {
    /// Create a team that you own, with you as its first member
    Create(TeamArgs),
    /// Add people to a team you own
    Add(MembersArgs),
    /// Remove people from a team you own, or leave a team you belong to
    Remove(RemoveMembersArgs),
    /// List the teams you belong to
    List(ListTeamsArgs),
    /// Show the members of teams you belong to
    Show(ShowTeamsArgs),
    /// Delete a team you own; its grants stay and open nothing, and its name is never used again
    Delete(DeleteTeamArgs),
}

/// The arguments that name one team.
#[derive(Debug, Args)]
pub(crate) struct TeamArgs {
    /// The team's name: 1 to 40 lowercase letters, digits and dashes
    #[arg(value_name = "NAME")]
    pub(crate) name: String,
}

/// The arguments of `flare teams add` and `flare teams remove`.
#[derive(Debug, Args)]
pub(crate) struct MembersArgs {
    /// The team's name
    #[arg(value_name = "NAME")]
    pub(crate) name: String,
    /// The people, repeated or separated by commas: each an address, or a name the server resolves
    #[arg(long, value_name = "PERSON", value_delimiter = ',', required = true)]
    pub(crate) with: Vec<String>,
}

/// The arguments of `flare teams remove`.
#[derive(Debug, Args)]
pub(crate) struct RemoveMembersArgs {
    /// The team and the people to remove from it.
    #[command(flatten)]
    pub(crate) members: MembersArgs,
    /// The confirmation that `--yes` gives without a question.
    #[command(flatten)]
    pub(crate) yes: Yes,
}

/// The arguments of `flare teams list`.
#[derive(Debug, Args)]
pub(crate) struct ListTeamsArgs {
    /// How many teams to list.
    #[command(flatten)]
    pub(crate) page: Page<50, 100>,
}

/// The arguments of `flare teams show`.
#[derive(Debug, Args)]
pub(crate) struct ShowTeamsArgs {
    /// The teams' names, up to 100
    #[arg(value_name = "NAME", required = true, num_args = 1..=MAX_SHOWN)]
    pub(crate) names: Vec<String>,
}

/// The arguments of `flare teams delete`.
#[derive(Debug, Args)]
pub(crate) struct DeleteTeamArgs {
    /// The team's name
    #[arg(value_name = "NAME")]
    pub(crate) name: String,
    /// The confirmation that `--yes` gives without a question.
    #[command(flatten)]
    pub(crate) yes: Yes,
}

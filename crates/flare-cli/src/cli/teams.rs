//! The arguments and help of `flare teams`.

use crate::documents::teams::{
    TEAMS_ADD, TEAMS_CREATE, TEAMS_DELETE, TEAMS_LIST, TEAMS_REMOVE, TEAMS_SHOW, TEAMS_TRANSFER,
};
use clap::{Args, Subcommand};
use crumbkit::help::{Effects, Example, Leaf};
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
    /// Hand a team you own to one of its members, who becomes its owner
    Transfer(TransferTeamArgs),
    /// Delete a team you own, removing its grant from every flare and freeing its name
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

/// The arguments of `flare teams transfer`.
#[derive(Debug, Args)]
pub(crate) struct TransferTeamArgs {
    /// The team's name
    #[arg(value_name = "NAME")]
    pub(crate) name: String,
    /// The member who becomes the owner: an address, or a name the server resolves
    #[arg(long, value_name = "PERSON")]
    pub(crate) to: String,
    /// The confirmation that `--yes` gives without a question.
    #[command(flatten)]
    pub(crate) yes: Yes,
}

/// The help of `flare teams create`.
pub(super) const CREATE_LEAF: Leaf = Leaf::new(&["teams", "create"])
    .examples(&[Example::new(&["design"], "Create the team design").needs(&["server"])])
    .effects(Effects::additive().idempotent())
    .document(TEAMS_CREATE);

/// The help of `flare teams add`.
pub(super) const ADD_LEAF: Leaf = Leaf::new(&["teams", "add"])
    .examples(&[Example::new(
        &["design", "--with", "mei@example.org,jo@example.org"],
        "Add two people to the team design",
    )
    .needs(&["server"])])
    .effects(Effects::additive())
    .document(TEAMS_ADD);

/// The help of `flare teams remove`.
pub(super) const REMOVE_LEAF: Leaf = Leaf::new(&["teams", "remove"])
    .examples(&[Example::new(
        &["design", "--with", "jo@example.org"],
        "Remove one person from the team design",
    )
    .needs(&["server"])])
    .effects(Effects::destructive())
    .document(TEAMS_REMOVE);

/// The help of `flare teams list`.
pub(super) const LIST_LEAF: Leaf = Leaf::new(&["teams", "list"])
    .examples(&[Example::new(&[], "List the teams you belong to").needs(&["server"])])
    .effects(Effects::read_only())
    .document(TEAMS_LIST);

/// The help of `flare teams show`.
pub(super) const SHOW_LEAF: Leaf = Leaf::new(&["teams", "show"])
    .examples(&[
        Example::new(&["design"], "Show the members of the team design").needs(&["server"]),
    ])
    .effects(Effects::read_only())
    .document(TEAMS_SHOW);

/// The help of `flare teams transfer`.
pub(super) const TRANSFER_LEAF: Leaf = Leaf::new(&["teams", "transfer"])
    .examples(&[Example::new(
        &["design", "--to", "jo@example.org", "--yes"],
        "Hand the team design to its member jo@example.org",
    )
    .needs(&["server"])])
    .effects(Effects::destructive().idempotent())
    .document(TEAMS_TRANSFER);

/// The help of `flare teams delete`.
pub(super) const DELETE_LEAF: Leaf = Leaf::new(&["teams", "delete"])
    .examples(&[Example::new(
        &["design", "--yes"],
        "Delete the team design without a question",
    )
    .needs(&["server"])])
    .effects(Effects::destructive().idempotent())
    .document(TEAMS_DELETE);

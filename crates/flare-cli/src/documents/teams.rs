//! The documents that `flare teams` prints.

use crumbkit::{Document, Listing, Row};
use schemars::JsonSchema;
use serde::Serialize;

/// One team.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct TeamRow {
    /// The team's name.
    #[row(column)]
    pub(crate) name: String,
    /// The owner's address.
    #[row(column)]
    pub(crate) owner: String,
    /// What you are to the team: `owner` or `member`.
    #[row(column)]
    pub(crate) role: String,
    /// How many members the team has, its owner included.
    #[row(column)]
    pub(crate) members: u32,
    /// When the team was created, in RFC 3339 UTC.
    #[row(long)]
    pub(crate) created: String,
}

/// The document of `flare teams create`.
pub(crate) const TEAMS_CREATE: Document<TeamRow> = Document::new(
    "flare",
    "teams-create",
    1,
    "The team that was created, with you as its owner and first member.",
);

/// The document of `flare teams list`.
pub(crate) const TEAMS_LIST: Document<Listing<TeamRow>> = Document::new(
    "flare",
    "teams-list",
    1,
    "The teams you belong to, by name.",
)
.when_empty("No teams.");

/// The people `flare teams add` or `flare teams remove` changed.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct MembersOut {
    /// The team's name.
    #[row(column)]
    pub(crate) team: String,
    /// How many members the team has now.
    #[row(column)]
    pub(crate) members: u32,
    /// The addresses added or removed; one already present, or already absent, is left out.
    #[row(column)]
    pub(crate) changed: Vec<String>,
}

/// The document of `flare teams add`.
pub(crate) const TEAMS_ADD: Document<MembersOut> = Document::new(
    "flare",
    "teams-add",
    1,
    "The team, its member count and the people this command added.",
);

/// The document of `flare teams remove`.
pub(crate) const TEAMS_REMOVE: Document<MembersOut> = Document::new(
    "flare",
    "teams-remove",
    1,
    "The team, its member count and the people this command removed.",
);

/// One member in `flare teams show`.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct MemberRow {
    /// The team's name.
    #[row(column)]
    pub(crate) team: String,
    /// The member's address.
    #[row(column)]
    pub(crate) member: String,
    /// `owner` for the team's owner, else `member`.
    #[row(column)]
    pub(crate) role: String,
}

/// The document of `flare teams show`.
pub(crate) const TEAMS_SHOW: Document<Listing<MemberRow>> = Document::new(
    "flare",
    "teams-show",
    1,
    "The members of a team you belong to, in address order.",
);

/// What `flare teams delete` did to one team.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct TeamDeletedRow {
    /// The team's name.
    #[row(column)]
    pub(crate) team: String,
    /// Whether the team is gone: its grant removed from every flare, its members removed and its name free. False only when the command stopped before the server finished; running it again goes on.
    #[row(column)]
    pub(crate) finished: bool,
    /// How many flares' grants to the team this command removed.
    #[row(column)]
    pub(crate) grants_removed: u32,
}

/// The document of `flare teams delete`.
pub(crate) const TEAMS_DELETE: Document<Listing<TeamDeletedRow>> = Document::new(
    "flare",
    "teams-delete",
    2,
    "The deleted team and how many flares' grants to it were removed. Once finished, its name is free for a new team, which gets none of the old team's access.",
);

/// The document of `flare teams transfer`.
pub(crate) const TEAMS_TRANSFER: Document<TeamRow> = Document::new(
    "flare",
    "teams-transfer",
    1,
    "The team after the transfer, with its new owner; your role is now member.",
);

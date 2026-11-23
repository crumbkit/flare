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

/// A team that `flare teams delete` deleted.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct TeamDeletedRow {
    /// The deleted team's name.
    #[row(column)]
    pub(crate) team: String,
    /// Always true: each flare's grant to the team stays and opens nothing until the flare's owner revokes it.
    #[row(column)]
    pub(crate) grants_dormant: bool,
    /// Always false: the name is never used again, so no new team can take over the dormant grants.
    #[row(column)]
    pub(crate) name_reusable: bool,
}

/// The document of `flare teams delete`.
pub(crate) const TEAMS_DELETE: Document<Listing<TeamDeletedRow>> = Document::new(
    "flare",
    "teams-delete",
    1,
    "The deleted team. Its grants stay dormant and its name is never used again.",
);

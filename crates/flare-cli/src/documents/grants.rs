//! The documents that `flare revoke` and `flare grants` print.

use crumbkit::{Document, Listing, Row};
use schemars::JsonSchema;
use serde::Serialize;

/// The grants that `flare revoke` ended.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct RevokedOut {
    /// The share's id.
    #[row(column)]
    pub(crate) id: String,
    /// The addresses, `team:<name>` and `everyone` whose grant ended; one that held no grant is left out.
    #[row(column)]
    pub(crate) revoked: Vec<String>,
}

/// The document of `flare revoke`.
pub(crate) const REVOKE: Document<RevokedOut> = Document::new(
    "flare",
    "revoke",
    1,
    "The share and the grants this command ended.",
);

/// One grant in `flare grants`.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct GrantRow {
    /// The person's address, `team:<name>`, `everyone`, or `team:*` for a team that you, as an Admin of the share, do not belong to.
    #[row(column)]
    pub(crate) grantee: String,
    /// Who the grant is to: `person`, `team` or `everyone`.
    #[row(column)]
    pub(crate) kind: String,
    /// The grant's role: `admin` or `reader`.
    #[row(column)]
    pub(crate) role: String,
    /// `active`, or `dormant` for a grant to a team that is being deleted, which opens nothing until the delete removes it.
    #[row(column, warning_by = |row: &Self| row.state == "dormant")]
    pub(crate) state: String,
    /// Who wrote the grant's current role.
    #[row(long)]
    pub(crate) granted_by: String,
    /// When the grant was first written, in RFC 3339 UTC.
    #[row(long)]
    pub(crate) created: String,
    /// For an everyone grant, the mail domain it opens to, or `admitted` for the server's email list.
    #[row(long)]
    pub(crate) audience: Option<String>,
}

/// The document of `flare grants`.
pub(crate) const GRANTS: Document<Listing<GrantRow>> = Document::new(
    "flare",
    "grants",
    1,
    "The grants of a share to people, teams and everyone, dormant grants of teams being deleted first.",
)
.when_empty("No grants.");

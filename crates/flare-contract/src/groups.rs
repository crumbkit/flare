//! The bodies of the routes that list and end grants, and of the team
//! routes.

use crate::wire::Role;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `DELETE /api/grants/{id}`: the grants to end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevokeRequest {
    /// Whose grant to end: each a person's identifier, which the deployment
    /// resolves, or `team:<name>`, or `everyone`.
    #[serde(default)]
    pub with: Vec<String>,
    /// End the grant to everyone signed in.
    #[serde(default)]
    pub everyone: bool,
}

/// The answer to `DELETE /api/grants/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Revoked {
    /// The share.
    pub share: String,
    /// The addresses, `team:<name>` and `everyone` whose grant ended. One
    /// that held no grant is left out.
    pub revoked: Vec<String>,
}

/// Who a grant is to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GranteeKind {
    /// One person.
    Person,
    /// The members of a team.
    Team,
    /// Everyone signed in whom the grant's audience covers.
    Everyone,
}

/// Whether a grant opens the share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GrantState {
    /// The grant opens the share to its grantee.
    Active,
    /// The grant is to a deleted team and opens nothing. The share's owner
    /// removes it with `flare revoke`.
    Dormant,
}

/// One grant of a share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GrantEntry {
    /// The person's identifier, `team:<name>` or `everyone`.
    pub grantee: String,
    /// Who the grant is to.
    pub kind: GranteeKind,
    /// The grant's role; a team or everyone grant is always `reader`.
    pub role: Role,
    /// Whether the grant opens the share.
    pub state: GrantState,
    /// Who wrote the grant's current role.
    pub granted_by: String,
    /// When the grant was first written, in RFC 3339 UTC.
    pub created: String,
    /// For an everyone grant, its audience: the mail domain it opens to, or
    /// `admitted` for the deployment's email list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
}

/// `GET /api/grants/{id}`: a share's grants, for its owner and its admins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GrantList {
    /// The share.
    pub share: String,
    /// The grants, oldest first.
    pub items: Vec<GrantEntry>,
}

/// `POST /api/teams`: a new team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TeamRequest {
    /// The name: 1 to 40 lowercase letters, digits and dashes, never used
    /// before in the deployment.
    pub name: String,
}

/// `POST` and `DELETE /api/teams/{name}/members`: the people to add or
/// remove.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MembersRequest {
    /// Each person's identifier, which the deployment resolves to an
    /// address.
    pub with: Vec<String>,
}

/// What the caller is to a team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TeamRole {
    /// The caller owns the team and alone manages it.
    Owner,
    /// The caller is a member.
    Member,
}

/// One team as a list or a reply shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamSummary {
    /// The team's name.
    pub name: String,
    /// The owner's address.
    pub owner: String,
    /// What the caller is to the team.
    pub role: TeamRole,
    /// How many members the team has, its owner included.
    pub members: u32,
    /// When the team was created, in RFC 3339 UTC.
    pub created: String,
}

/// `GET /api/teams`: the teams the caller belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamList {
    /// The teams, by name.
    pub items: Vec<TeamSummary>,
}

/// `GET /api/teams/{name}`: a team and its members, for a member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamShown {
    /// The team.
    pub team: TeamSummary,
    /// The members' addresses, in address order.
    pub members: Vec<String>,
}

/// The answer to adding or removing members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MembersChanged {
    /// The team after the change.
    pub team: TeamSummary,
    /// The addresses added or removed. One already present, or already
    /// absent, is left out.
    pub changed: Vec<String>,
}

/// The answer to `DELETE /api/teams/{name}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamDeleted {
    /// The deleted team's name.
    pub team: String,
    /// Always true: each share's grant to the team stays stored and opens
    /// nothing. The share's owner removes it with `flare revoke`.
    pub grants_dormant: bool,
    /// Always false: the name is never used again, so no new team can take
    /// over the dormant grants.
    pub name_reusable: bool,
}

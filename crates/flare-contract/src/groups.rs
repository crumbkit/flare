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
    /// The grant is to a team that is being deleted, or to a name no team
    /// has, and opens nothing. The team's delete removes it, and the
    /// share's owner may remove it with `flare revoke`.
    Dormant,
}

/// One grant of a share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GrantEntry {
    /// The person's identifier, `team:<name>` or `everyone`. A grant to a
    /// team that the caller, a share admin, does not belong to shows
    /// `team:*`, so no one outside a team learns its name.
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
    /// The name: 1 to 40 lowercase letters, digits and dashes, which no
    /// team in the deployment has.
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

/// Who keeps a team.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TeamManagedBy {
    /// flare keeps the team, and its owner changes it. A summary from a
    /// server that predates this field means this.
    #[default]
    Flare,
    /// The deployment's directory keeps the team. It has no owner, and
    /// flare cannot change it.
    Directory,
}

/// One team as a list or a reply shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamSummary {
    /// The team's name.
    pub name: String,
    /// The owner's identifier. A team that the deployment's directory
    /// keeps has none, and the field is left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Who keeps the team: `flare` or `directory`.
    #[serde(default)]
    pub managed_by: TeamManagedBy,
    /// What the caller is to the team. A directory team has only members.
    pub role: TeamRole,
    /// How many members the team has, its owner included.
    pub members: u32,
    /// When flare created the team, in RFC 3339 UTC. A directory team has
    /// none, and the field is left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
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

/// The answer to `DELETE /api/teams/{name}`. One call removes at most 200
/// of the team's grants; a team with more answers `finished` false, and
/// the owner repeats the call to go on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TeamDeleted {
    /// The team's name.
    pub team: String,
    /// Whether the team is gone: its grant removed from every share, its
    /// members removed and its name free for a new team. When false the
    /// team opens nothing and keeps its name until a repeat finishes.
    pub finished: bool,
    /// How many share grants this call removed.
    pub grants_removed: u32,
}

/// The body of `PUT /api/teams/{name}/owner`: the member who becomes the
/// team's owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransferRequest {
    /// The new owner's identifier, which the deployment resolves to an
    /// address. The person must be a member of the team.
    pub to: String,
}

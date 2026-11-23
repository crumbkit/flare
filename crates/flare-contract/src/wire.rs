//! The bodies that the JSON API's routes take and answer.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The markup of a share's content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// Markdown, which every deployment serves.
    Markdown,
    /// HTML, which the server refuses with `flare::shares::kind_not_served`
    /// until it serves uploaded HTML.
    Html,
}

/// What the caller is to a share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The caller owns it.
    Owner,
    /// The caller holds an Admin grant.
    Admin,
    /// The caller holds a Reader grant.
    Reader,
}

/// `POST /api/shares`: a new share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShareRequest {
    /// The title, 1 to 200 bytes.
    pub title: String,
    /// The markup of `content`.
    pub format: Format,
    /// The content, at most 256 KiB.
    pub content: String,
    /// Who to share with: each an identifier that the deployment resolves,
    /// which by default is an email address, or `team:<name>` for a team
    /// the caller belongs to, or `everyone`.
    #[serde(default)]
    pub with: Vec<String>,
    /// The ids of the flares this share responds to.
    #[serde(default)]
    pub re: Vec<String>,
}

/// `POST /api/shares/{id}/versions`: the next version of a share, which
/// only its owner may publish. A new version changes the content and
/// nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VersionRequest {
    /// The markup of `content`.
    pub format: Format,
    /// The content, at most 256 KiB.
    pub content: String,
}

/// `POST /api/grants/{id}`: more people, teams or everyone, or more flares
/// the share responds to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrantRequest {
    /// Who to share with: each a person's identifier, `team:<name>` for a
    /// team the caller belongs to, or `everyone`.
    #[serde(default)]
    pub with: Vec<String>,
    /// Grant Reader to everyone signed in whom the deployment's admit rule
    /// covers. Only the share's owner may. A body without it grants no
    /// everyone grant.
    #[serde(default, skip_serializing_if = "is_false")]
    pub everyone: bool,
    /// The ids of more flares the share responds to. Only its owner names
    /// them.
    #[serde(default)]
    pub re: Vec<String>,
}

/// Whether `value` is false, so a false flag is left out of a body.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes the field by reference"
)]
fn is_false(value: &bool) -> bool {
    !*value
}

/// One share as a list or a reply shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ShareSummary {
    /// The share's id.
    pub id: String,
    /// The title.
    pub title: String,
    /// The markup.
    pub format: Format,
    /// The owner's identifier.
    pub owner: String,
    /// What the caller is to the share.
    pub role: Role,
    /// The latest version.
    pub version: u32,
    /// When the share was created, in RFC 3339 UTC.
    pub created: String,
}

/// `GET /api/shares/{id}`: the latest version of a share the caller can
/// open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpenedShare {
    /// The share.
    pub share: ShareSummary,
    /// The latest version's content.
    pub content: String,
}

/// The answer to `POST /api/shares` and `POST /api/shares/{id}/grants`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Shared {
    /// The share.
    pub share: ShareSummary,
    /// The addresses, `team:<name>` and `everyone` granted Reader by this
    /// request.
    pub granted: Vec<String>,
    /// The flares the share responds to that this request named.
    pub responds_to: Vec<String>,
}

/// `GET /api/shares?scope=mine|with-me&limit=N`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ShareList {
    /// The shares, newest first.
    pub items: Vec<ShareSummary>,
    /// Whether more shares exist than `limit` let through.
    pub more: bool,
}

/// A flare that a response also responds to, which the caller can open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FlareRef {
    /// The flare's id.
    pub id: String,
    /// Its title.
    pub title: String,
}

/// One item of the activity feed, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[non_exhaustive]
pub enum ActivityItem {
    /// A share that responds to a flare with `--re`.
    Response {
        /// The flare it responds to.
        flare: String,
        /// The responding share's id.
        share: String,
        /// Its title.
        title: String,
        /// Its markup.
        format: Format,
        /// Its owner's identifier.
        by: String,
        /// The version of the flare it responds to.
        version: u32,
        /// When it named the flare, in RFC 3339 UTC.
        at: String,
        /// The other flares it responds to that the caller can open.
        also: Vec<FlareRef>,
    },
}

/// `GET /api/activity?flare=<id>&since=<cursor>&limit=N`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ActivityPage {
    /// The items, oldest first.
    pub items: Vec<ActivityItem>,
    /// The cursor to pass as `since` next time.
    pub cursor: String,
    /// Whether more items follow the cursor now.
    pub more: bool,
}

/// `POST /api/tokens/exchange`: a login code for a token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExchangeRequest {
    /// The one-use code the browser handed to the CLI.
    pub code: String,
    /// The token's label, such as the name of the Mac.
    pub label: String,
}

/// The answer to an exchange. The token appears here once and nowhere
/// else; the server keeps only its digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TokenIssued {
    /// The token's id, which `flare logout` revokes.
    pub id: String,
    /// The bearer token.
    pub token: String,
    /// The address it authenticates as.
    pub owner: String,
    /// Its label.
    pub label: String,
    /// When it stops working, in RFC 3339 UTC.
    pub expires: String,
}

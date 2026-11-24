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
    /// How many live comments and replies the flare holds. Only an opened
    /// share carries it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comments: Option<u32>,
}

/// `GET /api/shares/{id}?version=n`: version `n` of a share the caller can
/// open, or its latest version without `n`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpenedShare {
    /// The share, whose `version` is its latest.
    pub share: ShareSummary,
    /// The number of the version whose content this answer holds.
    pub version: u32,
    /// That version's content.
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

/// `GET /api/shares?scope=mine|with-me&limit=N&cursor=<cursor>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ShareList {
    /// The shares, newest first.
    pub items: Vec<ShareSummary>,
    /// Whether more shares follow this page; true exactly when `next` is
    /// present.
    pub more: bool,
    /// The cursor to pass as `cursor` for the page that follows, present
    /// only when more shares follow. A page can hold fewer than `limit`
    /// shares and still name one, when a share granted to the caller no
    /// longer exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
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
    /// A comment on a flare.
    Comment {
        /// The flare.
        flare: String,
        /// The comment's id.
        comment: String,
        /// Its author's identifier.
        by: String,
        /// Its text.
        body: String,
        /// When it was written, in RFC 3339 UTC.
        at: String,
    },
    /// A reply to a comment.
    Reply {
        /// The flare.
        flare: String,
        /// The reply's id.
        comment: String,
        /// The comment it answers.
        parent: String,
        /// Its author's identifier.
        by: String,
        /// Its text.
        body: String,
        /// When it was written, in RFC 3339 UTC.
        at: String,
    },
    /// A comment or reply whose author changed its text.
    Edited {
        /// The flare.
        flare: String,
        /// The comment's id.
        comment: String,
        /// The comment a reply answers; none for a comment.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        /// Its author's identifier.
        by: String,
        /// Its text now.
        body: String,
        /// When it changed, in RFC 3339 UTC.
        at: String,
    },
    /// A comment or reply whose author deleted it.
    Deleted {
        /// The flare.
        flare: String,
        /// The comment's id.
        comment: String,
        /// The comment a reply answers; none for a comment.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<String>,
        /// Its author's identifier.
        by: String,
        /// When it was deleted, in RFC 3339 UTC.
        at: String,
    },
}

/// A top-level comment with its replies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Comment {
    /// The comment's id.
    pub id: String,
    /// The flare it is on.
    pub flare: String,
    /// Its author's identifier.
    pub author: String,
    /// When it was written, in RFC 3339 UTC.
    pub at: String,
    /// When its text last changed, in RFC 3339 UTC; absent when it never did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited: Option<String>,
    /// Whether its author deleted it. A deleted comment has no body.
    pub deleted: bool,
    /// Its text, absent once deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Its replies, oldest first.
    pub replies: Vec<Reply>,
}

/// A reply to a comment. A reply has no replies of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Reply {
    /// The reply's id.
    pub id: String,
    /// The flare it is on.
    pub flare: String,
    /// The comment it answers.
    pub parent: String,
    /// Its author's identifier.
    pub author: String,
    /// When it was written, in RFC 3339 UTC.
    pub at: String,
    /// When its text last changed, in RFC 3339 UTC; absent when it never did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited: Option<String>,
    /// Whether its author deleted it. A deleted reply has no body.
    pub deleted: bool,
    /// Its text, absent once deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// `GET /api/comments?flare=<id>`: a flare's comments with their replies,
/// oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CommentThread {
    /// The flare.
    pub flare: String,
    /// Its comments.
    pub comments: Vec<Comment>,
}

/// The answer to writing, editing or deleting one comment or reply, tagged
/// by `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[non_exhaustive]
pub enum CommentOrReply {
    /// A comment, with no replies in a write's answer.
    Comment(Comment),
    /// A reply.
    Reply(Reply),
}

/// `POST /api/comments`: a comment on a flare, or a reply to one of its
/// comments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NewComment {
    /// The flare.
    pub flare: String,
    /// The comment to reply to; absent for a comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// The text, 1 to 8,000 characters.
    pub body: String,
    /// The client's key for this write, 1 to 64 characters of letters,
    /// digits, `.`, `_` and `-`. Sending the same key again answers the
    /// first comment and writes nothing.
    pub key: String,
}

/// `PATCH /api/comments/{id}`: the new text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommentEdit {
    /// The text, 1 to 8,000 characters.
    pub body: String,
}

/// `GET` and `PUT /api/cursors/{name}`: a named position in the activity
/// feed, kept for the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamedCursor {
    /// The activity cursor. A `PUT` moves the saved one only when this is
    /// further on, and the answer holds the cursor saved.
    pub cursor: String,
}

/// `GET /api/activity?flare=<id>&since=<cursor>&since_last=<name>&limit=N`.
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

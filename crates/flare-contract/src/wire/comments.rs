//! The bodies of the comment routes: a flare's thread, a new comment or
//! reply, an edit, and the page text a comment is about.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The text on a Markdown flare that a comment is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    /// The selected page text, 3 to 500 characters, with each run of
    /// whitespace as one space.
    pub quote: String,
    /// Up to 64 characters of page text just before the quote.
    pub prefix: String,
    /// Up to 64 characters of page text just after the quote.
    pub suffix: String,
    /// The version of the flare the text was selected on.
    pub version: u32,
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
    /// The page text it is about; absent for a comment on the whole flare
    /// and once deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
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
    /// The page text a comment is about; absent for a comment on the whole
    /// flare. A reply and a comment on a flare that is not Markdown are
    /// refused with one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
}

/// `PATCH /api/comments/{id}`: the new text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommentEdit {
    /// The text, 1 to 8,000 characters.
    pub body: String,
}

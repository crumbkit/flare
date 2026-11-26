//! The documents that `flare`'s commands print (CLI-2). None holds a
//! token, a login code, a launch host or a launch URL.

mod grants;
mod metrics;
mod shares;
pub(crate) mod teams;

pub(crate) use grants::{GRANTS, GrantRow, REVOKE, RevokedOut};
pub(crate) use metrics::{METRICS, MetricRow, MetricTotal, MetricTotals};
pub(crate) use shares::{GRANT, SHARE, SHOW, ShareOut, SharedOut, Shown};

use crumbkit::{Document, Listing, Row};
use schemars::JsonSchema;
use serde::Serialize;

/// What `flare serve` reports when it stops.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct Served {
    /// The board's directory.
    #[row(column)]
    pub(crate) board: String,
    /// Why the server stopped: `shutdown` after SIGTERM, `interrupted`
    /// after Ctrl-C or `idle` after the idle period.
    #[row(column)]
    pub(crate) stopped: &'static str,
}

/// The document of `flare serve`.
pub(crate) const SERVE: Document<Served> = Document::new(
    "flare",
    "serve",
    1,
    "The board a server served and why it stopped.",
);

/// What `flare open` opened.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct Opened {
    /// The path the browser opens on the server, such as `/b/kitchen`.
    #[row(column)]
    pub(crate) target: String,
}

/// The document of `flare open`.
pub(crate) const OPEN: Document<Opened> = Document::new(
    "flare",
    "open",
    1,
    "The page, binding or index that the browser opened.",
);

/// What `flare publish` published, or would publish.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct Published {
    /// The page's name.
    #[row(column)]
    pub(crate) name: String,
    /// The new version, or none on a dry run.
    #[row(column)]
    pub(crate) version: Option<String>,
    /// The page's markup language: `html` or `markdown`.
    #[row(column)]
    pub(crate) format: &'static str,
    /// The names of the page's slots.
    #[row(column)]
    pub(crate) slots: Vec<String>,
}

/// The document of `flare publish`.
pub(crate) const PUBLISH: Document<Published> = Document::new(
    "flare",
    "publish",
    1,
    "The page that was published, with its new version.",
);

/// What `flare bind` saved, or would save.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct Bound {
    /// The binding's name.
    #[row(column)]
    pub(crate) name: String,
    /// The page the binding fills.
    #[row(column)]
    pub(crate) page: String,
    /// The page version the binding is pinned to.
    #[row(column)]
    pub(crate) version: String,
}

/// The document of `flare bind`.
pub(crate) const BIND: Document<Bound> = Document::new(
    "flare",
    "bind",
    1,
    "The binding that was saved and the page version it is pinned to.",
);

/// What `flare login` saved.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct LoggedIn {
    /// The server's origin.
    #[row(column)]
    pub(crate) server: String,
    /// The address the token signs in as.
    #[row(column)]
    pub(crate) owner: String,
    /// The token's label.
    #[row(column)]
    pub(crate) label: String,
    /// When the token expires, in RFC 3339 UTC.
    #[row(column)]
    pub(crate) expires: String,
}

/// The document of `flare login`.
pub(crate) const LOGIN: Document<LoggedIn> = Document::new(
    "flare",
    "login",
    1,
    "The server, the person and the label of the token that the Keychain now holds.",
);

/// What `flare logout` forgot.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct LoggedOut {
    /// The server's origin.
    #[row(column)]
    pub(crate) server: String,
    /// The label of the revoked token, which is empty when nothing was
    /// revoked.
    #[row(column)]
    pub(crate) label: String,
    /// Whether a saved token was revoked and deleted. It is false when no
    /// token was saved, as with a token command and no earlier login.
    #[row(column)]
    pub(crate) revoked: bool,
}

/// The document of `flare logout`.
pub(crate) const LOGOUT: Document<LoggedOut> = Document::new(
    "flare",
    "logout",
    1,
    "The server and the label of the token that was revoked and deleted, and whether one was.",
);

/// One share in `flare shares`.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct ShareRow {
    /// The share's id.
    #[row(column)]
    pub(crate) id: String,
    /// The share's title.
    #[row(column)]
    pub(crate) title: String,
    /// The owner's identifier.
    #[row(column)]
    pub(crate) owner: String,
    /// The caller's role: `owner`, `admin` or `reader`.
    #[row(column)]
    pub(crate) role: String,
    /// The latest version.
    #[row(column)]
    pub(crate) version: u32,
    /// When it was created, in RFC 3339 UTC.
    #[row(column)]
    pub(crate) created: String,
}

/// Where the next `flare shares` starts.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct SharesNext {
    /// The cursor to pass as `--cursor` for the shares that follow, or none
    /// when the list ends here.
    pub(crate) cursor: Option<String>,
}

/// The text output prints no summary line: the listing's step names the
/// next `flare shares` call with the cursor, and the JSON document keeps
/// `cursor` under `summary`.
impl std::fmt::Display for SharesNext {
    fn fmt(&self, _formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

/// The document of `flare shares`.
pub(crate) const SHARES: Document<Listing<ShareRow, SharesNext>> = Document::new(
    "flare",
    "shares",
    1,
    "The caller's shares, or the shares granted to the caller, newest first, and the cursor of the shares that follow.",
)
.when_empty("No shares.");

/// One response in `flare activity`.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct ActivityRow {
    /// The flare it responds to.
    #[row(column)]
    pub(crate) flare: String,
    /// What happened: `response`, `comment`, `reply`, `edited` or `deleted`.
    #[row(column)]
    pub(crate) kind: String,
    /// The responding share's id, for a response.
    #[row(column)]
    pub(crate) share: Option<String>,
    /// The responding share's title, for a response.
    #[row(column)]
    pub(crate) title: Option<String>,
    /// The comment or reply the event is about, for a comment event.
    #[row(column)]
    pub(crate) comment: Option<String>,
    /// The comment a reply answers, or an edited or deleted reply is on.
    #[row(column)]
    pub(crate) parent: Option<String>,
    /// The person who responded, wrote, edited or deleted.
    #[row(column)]
    pub(crate) by: String,
    /// The version of the flare it responds to, for a response.
    #[row(column)]
    pub(crate) version: Option<u32>,
    /// The comment's or reply's text after the event; absent for a response or a deletion.
    #[row(column)]
    pub(crate) body: Option<String>,
    /// When it happened, in RFC 3339 UTC.
    #[row(column)]
    pub(crate) at: String,
    /// The other flares a response responds to that the caller can open.
    #[row(column)]
    pub(crate) also: Vec<String>,
}

/// Where the next `flare activity` starts.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct Next {
    /// The cursor to pass as `--since`.
    pub(crate) cursor: String,
    /// The name this call saved the cursor under, when it ran with `--since-last`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) saved_as: Option<String>,
    /// Whether newer responses wait past this page.
    pub(crate) more: bool,
}

/// The text output prints no summary line: the listing's step names the
/// next `flare activity` call with the saved cursor name or the cursor, and
/// the JSON document keeps `cursor`, `saved_as` and `more` under `summary`.
impl std::fmt::Display for Next {
    fn fmt(&self, _formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

/// The document of `flare activity`.
pub(crate) const ACTIVITY: Document<Listing<ActivityRow, Next>> = Document::new(
    "flare",
    "activity",
    1,
    "The responses and comment events on the named flares, oldest first, and the cursor of the next call.",
)
.when_empty("No new activity.");

/// One comment or reply, as `flare comment`, `reply`, `comment-edit` and
/// `comment-delete` print it.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct CommentOut {
    /// The comment's or reply's id.
    #[row(column)]
    pub(crate) id: String,
    /// The flare it is on.
    #[row(column)]
    pub(crate) flare: String,
    /// The comment a reply answers; absent for a comment.
    #[row(column)]
    pub(crate) parent: Option<String>,
    /// The author's identifier.
    #[row(column)]
    pub(crate) author: String,
    /// When it was written, in RFC 3339 UTC.
    #[row(column)]
    pub(crate) at: String,
    /// When its text last changed, in RFC 3339 UTC; absent when it never did.
    #[row(column)]
    pub(crate) edited: Option<String>,
    /// Whether its author deleted it.
    #[row(column)]
    pub(crate) deleted: bool,
    /// Its text; absent once deleted.
    #[row(column)]
    pub(crate) body: Option<String>,
}

/// The document of `flare comment`.
pub(crate) const COMMENT: Document<CommentOut> = Document::new(
    "flare",
    "comment",
    1,
    "The comment written. A repeated --key answers the first comment and writes nothing.",
);

/// The document of `flare reply`.
pub(crate) const REPLY: Document<CommentOut> = Document::new(
    "flare",
    "reply",
    1,
    "The reply written. A repeated --key answers the first reply and writes nothing.",
);

/// The document of `flare comment-edit`.
pub(crate) const COMMENT_EDIT: Document<CommentOut> = Document::new(
    "flare",
    "comment-edit",
    1,
    "The comment or reply with its new text.",
);

/// The document of `flare comment-delete`.
pub(crate) const COMMENT_DELETE: Document<CommentOut> = Document::new(
    "flare",
    "comment-delete",
    1,
    "The deleted comment or reply, which no longer has its text.",
);

/// One comment or reply in `flare comments`.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct CommentRow {
    /// The comment's or reply's id.
    #[row(column)]
    pub(crate) id: String,
    /// The comment a reply answers; absent for a comment.
    #[row(column)]
    pub(crate) parent: Option<String>,
    /// The author's identifier.
    #[row(column)]
    pub(crate) author: String,
    /// When it was written, in RFC 3339 UTC.
    #[row(column)]
    pub(crate) at: String,
    /// When its text last changed, in RFC 3339 UTC.
    #[row(column)]
    pub(crate) edited: Option<String>,
    /// Whether its author deleted it.
    #[row(column)]
    pub(crate) deleted: bool,
    /// Its text; absent once deleted.
    #[row(column)]
    pub(crate) body: Option<String>,
}

/// The document of `flare comments`.
pub(crate) const COMMENTS: Document<Listing<CommentRow>> = Document::new(
    "flare",
    "comments",
    1,
    "The flare's comments, oldest first, each followed by its replies, oldest first.",
)
.when_empty("No comments.");

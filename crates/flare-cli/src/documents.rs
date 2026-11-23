//! The documents that `flare`'s commands print (CLI-2). None holds a
//! token, a login code, a launch host or a launch URL.

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
    /// The label of the revoked token.
    #[row(column)]
    pub(crate) label: String,
}

/// The document of `flare logout`.
pub(crate) const LOGOUT: Document<LoggedOut> = Document::new(
    "flare",
    "logout",
    1,
    "The server and the label of the token that was revoked and deleted.",
);

/// A share that `flare share` made or `flare grant` granted.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct SharedOut {
    /// The share's id.
    #[row(column)]
    pub(crate) id: String,
    /// The share's title.
    #[row(column)]
    pub(crate) title: String,
    /// The addresses that now hold a role on it.
    #[row(column)]
    pub(crate) granted: Vec<String>,
    /// The flares it responds to.
    #[row(column)]
    pub(crate) responds_to: Vec<String>,
}

/// The document of `flare share`.
pub(crate) const SHARE: Document<SharedOut> = Document::new(
    "flare",
    "share",
    1,
    "The new share, the people it is granted to and the flares it responds to.",
);

/// The document of `flare grant`.
pub(crate) const GRANT: Document<SharedOut> = Document::new(
    "flare",
    "grant",
    1,
    "The share, the people this grant added and the flares it now responds to.",
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

/// The document of `flare shares`.
pub(crate) const SHARES: Document<Listing<ShareRow>> = Document::new(
    "flare",
    "shares",
    1,
    "The caller's shares, or the shares granted to the caller, newest first.",
)
.when_empty("No shares.");

/// One response in `flare activity`.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct ActivityRow {
    /// The flare it responds to.
    #[row(column)]
    pub(crate) flare: String,
    /// The responding share's id.
    #[row(column)]
    pub(crate) share: String,
    /// The responding share's title.
    #[row(column)]
    pub(crate) title: String,
    /// The responder's identifier.
    #[row(column)]
    pub(crate) by: String,
    /// The version of the flare it responds to.
    #[row(column)]
    pub(crate) version: u32,
    /// When it named the flare, in RFC 3339 UTC.
    #[row(column)]
    pub(crate) at: String,
    /// The other flares it responds to that the caller can open.
    #[row(column)]
    pub(crate) also: Vec<String>,
}

/// Where the next `flare activity` starts.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct Next {
    /// The cursor to pass as `--since`.
    pub(crate) cursor: String,
    /// Whether newer responses wait past this page.
    pub(crate) more: bool,
}

impl std::fmt::Display for Next {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "Next: --since {}", self.cursor)?;
        if self.more {
            formatter.write_str(" (more responses wait)")?;
        }
        Ok(())
    }
}

/// The document of `flare activity`.
pub(crate) const ACTIVITY: Document<Listing<ActivityRow, Next>> = Document::new(
    "flare",
    "activity",
    1,
    "The responses to the named flares, oldest first, and the cursor of the next call.",
)
.when_empty("No new responses.");

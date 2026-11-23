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

/// A share that `flare share` made or updated, or `flare grant` granted.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct SharedOut {
    /// The share's id.
    #[row(column)]
    pub(crate) id: String,
    /// The share's title.
    #[row(column)]
    pub(crate) title: String,
    /// The share's latest version.
    #[row(column)]
    pub(crate) version: u32,
    /// The addresses that now hold a role on it; none after `--update`, which does not report them.
    #[row(column)]
    pub(crate) granted: Option<Vec<String>>,
    /// The flares it responds to; none after `--update`, which does not report them.
    #[row(column)]
    pub(crate) responds_to: Option<Vec<String>>,
}

/// A share that `flare share` made or updated, or that a dry run checked.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct ShareOut {
    /// The share's id; none on a dry run of a new share.
    #[row(column)]
    pub(crate) id: Option<String>,
    /// The share's title; none on a dry run of `--update`, which does not read the share.
    #[row(column)]
    pub(crate) title: Option<String>,
    /// The share's latest version; none on a dry run.
    #[row(column)]
    pub(crate) version: Option<u32>,
    /// The addresses that now hold a role on it; none after `--update` or a dry run.
    #[row(column)]
    pub(crate) granted: Option<Vec<String>>,
    /// The flares it responds to; none after `--update` or a dry run.
    #[row(column)]
    pub(crate) responds_to: Option<Vec<String>>,
    /// What a dry run checked, wrote and could not check; empty on a real run.
    #[row(children(help = "A dry run lists its checks."))]
    pub(crate) checks: Vec<String>,
}

/// The document of `flare share`.
pub(crate) const SHARE: Document<ShareOut> = Document::new(
    "flare",
    "share",
    1,
    "The share, its latest version, the people it is granted to and the flares it responds to, or what a dry run checked.",
);

/// One version of a share that `flare show` read.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct Shown {
    /// The share's id.
    #[row(column, uncut)]
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
    /// The version whose content this holds.
    #[row(column)]
    pub(crate) version: u32,
    /// The share's latest version.
    #[row(column)]
    pub(crate) latest: u32,
    /// The version's Markdown.
    #[row(children(lines = content_lines, help = "The version's Markdown follows the share, one line of text for each line of the file."))]
    pub(crate) content: String,
}

/// The lines of a shown version's Markdown.
fn content_lines(shown: &Shown) -> Vec<String> {
    shown.content.lines().map(str::to_owned).collect()
}

/// The document of `flare show`.
pub(crate) const SHOW: Document<Shown> = Document::new(
    "flare",
    "show",
    1,
    "One version of a share the caller can open: its title, owner, the caller's role, its number and its Markdown.",
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

/// The count of one metric on one day in `flare metrics`.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct MetricRow {
    /// The day, as `YYYY-MM-DD` in UTC.
    #[row(column)]
    pub(crate) day: String,
    /// The metric's name, such as `PagesViewed`.
    #[row(column)]
    pub(crate) metric: String,
    /// The number of events the metric counted that day.
    #[row(column)]
    pub(crate) count: u64,
}

/// The count of one metric over every day in `flare metrics`.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct MetricTotal {
    /// The metric's name.
    pub(crate) metric: String,
    /// The number of events the metric counted.
    pub(crate) count: u64,
}

/// The totals of `flare metrics` and the lines it read.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct MetricTotals {
    /// The count of each metric over every day, ordered by name.
    pub(crate) totals: Vec<MetricTotal>,
    /// The number of lines read.
    pub(crate) lines: u64,
    /// The number of lines skipped because they are not Flare metric lines.
    pub(crate) skipped: u64,
}

impl std::fmt::Display for MetricTotals {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for total in &self.totals {
            writeln!(formatter, "{}: {}", total.metric, total.count)?;
        }
        write!(
            formatter,
            "Read {} lines and skipped {} that are not Flare metric lines.",
            self.lines, self.skipped
        )
    }
}

/// The document of `flare metrics`.
pub(crate) const METRICS: Document<Listing<MetricRow, MetricTotals>> = Document::new(
    "flare",
    "metrics",
    1,
    "The count of each Flare metric on each day in UTC, the total of each metric, and the lines read and skipped.",
)
.when_empty("No Flare metric lines.");

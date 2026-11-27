//! The documents that `flare share`, `show` and `grant` print.

use crumbkit::{Document, Row};
use schemars::JsonSchema;
use serde::Serialize;

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
    /// The addresses, `team:<name>` and `everyone` that now hold a role on it; none after `--update`, which does not report them.
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
    /// The images sent with the document; on a dry run, the images it would send.
    #[row(column)]
    pub(crate) images: usize,
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
    /// The share's markup: `markdown`, or `html` for an uploaded HTML file.
    #[row(column)]
    pub(crate) format: String,
    /// The version's Markdown. An HTML share's is empty, because the reading
    /// page serves its file only in a sandboxed frame.
    #[row(children(lines = content_lines, help = "The version's Markdown follows the share, one line of text for each line of the file. An HTML share prints a line that says so instead."))]
    pub(crate) content: String,
}

/// What `flare show` prints in place of an HTML share's empty Markdown.
const HTML_NOTE: &str = "An HTML share: the reading page shows its file in a sandboxed frame.";

/// The lines of a shown version's Markdown, or the note of an HTML share.
fn content_lines(shown: &Shown) -> Vec<String> {
    if shown.format == "html" {
        return vec![HTML_NOTE.to_owned()];
    }
    shown.content.lines().map(str::to_owned).collect()
}

/// The document of `flare show`.
pub(crate) const SHOW: Document<Shown> = Document::new(
    "flare",
    "show",
    1,
    "One version of a share the caller can open: its title, owner, the caller's role, its number, its format and its Markdown.",
);

/// The document of `flare grant`.
pub(crate) const GRANT: Document<SharedOut> = Document::new(
    "flare",
    "grant",
    1,
    "The share, the people this grant added and the flares it now responds to.",
);

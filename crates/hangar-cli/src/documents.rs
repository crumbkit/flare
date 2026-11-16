//! The documents that `hangar`'s commands print (CLI-2). None holds a
//! token, a launch host or a launch URL.

use crumbkit::{Document, Row};
use schemars::JsonSchema;
use serde::Serialize;

/// What `hangar serve` reports when it stops.
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

/// The document of `hangar serve`.
pub(crate) const SERVE: Document<Served> = Document::new(
    "hangar",
    "serve",
    1,
    "The board a server served and why it stopped.",
);

/// What `hangar open` opened.
#[derive(Debug, Serialize, JsonSchema, Row)]
pub(crate) struct Opened {
    /// The path the browser opens on the server, such as `/b/kitchen`.
    #[row(column)]
    pub(crate) target: String,
}

/// The document of `hangar open`.
pub(crate) const OPEN: Document<Opened> = Document::new(
    "hangar",
    "open",
    1,
    "The page, binding or index that the browser opened.",
);

/// What `hangar publish` published, or would publish.
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

/// The document of `hangar publish`.
pub(crate) const PUBLISH: Document<Published> = Document::new(
    "hangar",
    "publish",
    1,
    "The page that was published, with its new version.",
);

/// What `hangar bind` saved, or would save.
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

/// The document of `hangar bind`.
pub(crate) const BIND: Document<Bound> = Document::new(
    "hangar",
    "bind",
    1,
    "The binding that was saved and the page version it is pinned to.",
);

//! The grammar and help of `flare schemas list` and `flare schemas show`,
//! which crumbkit's schema commands serve.

use clap::Subcommand;
use crumbkit::help::{Effects, Example, Leaf};
use crumbkit::schemas::{SCHEMAS_LIST, SCHEMAS_SHOW, SchemasList, SchemasShow};

/// The commands for the JSON documents that flare prints.
#[derive(Debug, Subcommand)]
pub(crate) enum SchemasCommand {
    /// List the JSON documents that flare prints, with a description of each
    List(SchemasList),
    /// Print the JSON Schema of each document
    Show(SchemasShow),
}

/// The help of `flare schemas list`.
pub(super) const LIST_LEAF: Leaf = Leaf::new(&["schemas", "list"])
    .examples(&[Example::new(&[], "List every document that flare prints")])
    .effects(Effects::read_only())
    .document(SCHEMAS_LIST);

/// The help of `flare schemas show`.
pub(super) const SHOW_LEAF: Leaf = Leaf::new(&["schemas", "show"])
    .examples(&[Example::new(
        &["flare/shares/v1"],
        "Print the JSON Schema of the document that flare shares prints",
    )])
    .effects(Effects::read_only())
    .document(SCHEMAS_SHOW);

//! The grammar and help of `flare search`.

use crate::search::SEARCH;
use clap::Args;
use crumbkit::Page;
use crumbkit::help::{Effects, Example, Leaf};

/// The arguments of `flare search`.
#[derive(Debug, Args)]
pub(crate) struct SearchArgs {
    /// The search, such as 'budget -type:html' or 'from:me "review notes"'.
    /// It may start with a minus, as in '-from:me'.
    #[arg(value_name = "QUERY", allow_hyphen_values = true)]
    pub(crate) query: String,
    /// How many shares to list; --all lists up to 500.
    #[command(flatten)]
    pub(crate) page: Page<50, 500>,
}

/// The help of `flare search`.
pub(super) const LEAF: Leaf = Leaf::new(&["search"])
    .examples(&[
        Example::new(
            &["budget -type:html"],
            "List the shares you can open whose title holds budget, leaving out HTML shares",
        )
        .needs(&["server"]),
        Example::new(
            &["from:ana@example.org,me \"review notes\"", "--limit", "20"],
            "List the 20 newest shares from ana or you whose title holds review notes",
        )
        .needs(&["server"]),
    ])
    .effects(Effects::read_only())
    .document(SEARCH);

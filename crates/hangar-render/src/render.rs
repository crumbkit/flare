//! Renders a page and the records that fill it into one HTML document.

mod data;
mod walk;

use crate::check::{TemplateFlaws, template};
use crate::html::{Html, stylesheet};
use crate::markdown;
use hangar_core::{Misfit, Page, Record, SlotName};
use mustache2::Data;
use mustache2::render::provider::SourceProvider;
use mustache2::render::{RenderManager, SourceCache};
use std::borrow::Cow;

/// The largest rendered output in bytes, for the worst case of a template's
/// sections and the data's lists, and for a Markdown page's HTML.
pub(crate) const MAX_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

/// The most bytes that mustache2 may copy and look up while it renders one
/// page. A copied value counts its text and 32 bytes more, so one copy of a
/// record of 1 MiB counts from about 1 MiB to about 16 MiB. A lookup counts
/// the name's length times the depth of the context stack. In a release
/// build, rendering that copies just under 64 MiB took about 100
/// milliseconds, and a 256-byte name looked up 100,000 times took about 45.
pub(crate) const MAX_COPIED_BYTES: usize = 64 * 1024 * 1024;

/// A whole HTML document for the sandboxed frame: the stylesheet in a
/// `<style>` element, then the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageDocument(String);

impl PageDocument {
    /// The document's markup.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The document's markup, as an owned string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

/// Why a page could not be rendered. Each leaves no HTML.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RenderError {
    /// The page's template has flaws. A page that passed `check_page` at
    /// publish can still reach a store by a plain commit.
    #[error("the page's template has flaws")]
    Flaws(#[source] TemplateFlaws),
    /// A fill names an undeclared slot or has the wrong type.
    #[error("a record does not fit the page")]
    Misfit(#[source] Misfit),
    /// The body of the record or sample in a slot is not one JSON object.
    #[error("the record in slot {slot} is not a JSON object")]
    NotAnObject {
        /// The slot.
        slot: SlotName,
    },
    /// The worst-case output is larger than 4 MiB.
    #[error("the page would render to more than {max} bytes")]
    TooLarge {
        /// The largest output in bytes.
        max: usize,
    },
    /// Rendering would copy or look up more than 64 MiB of record data and
    /// names, because sections push large values or tags name long names
    /// inside long loops.
    #[error("rendering the page would copy or look up more than {max} bytes")]
    TooCostly {
        /// The most record data in bytes that rendering may copy.
        max: usize,
    },
    /// A variable tag names a list or an object, which Mustache would write
    /// without escaping.
    #[error(
        "the tag {name} at line {line}, column {column} names a list or an object; put it in a section"
    )]
    NotText {
        /// The name in the tag.
        name: String,
        /// The tag's line, counted from 1.
        line: usize,
        /// The tag's column, counted in characters from 1.
        column: usize,
    },
    /// mustache2 refused the template.
    #[error("the template could not be rendered: {message}")]
    Mustache {
        /// mustache2's message.
        message: String,
    },
}

/// Renders `page` with the records in `fills` into a whole HTML document.
///
/// The template check runs again first. A slot that `fills` leaves out shows
/// its sample, and a slot with neither shows nothing. A Markdown page is not
/// a template: it renders the same with any fills.
///
/// # Errors
///
/// Returns [`RenderError::Flaws`] for a template the check refuses,
/// [`RenderError::Misfit`] for a fill of an undeclared slot or of the wrong
/// type, [`RenderError::NotAnObject`] for a body that is not one JSON
/// object, [`RenderError::TooLarge`] when the output could exceed 4 MiB,
/// [`RenderError::TooCostly`] when rendering would copy or look up more
/// than 64 MiB, [`RenderError::NotText`] for a variable tag that names a list or an
/// object, and [`RenderError::Mustache`] when mustache2 refuses the
/// template.
pub fn render_page(page: &Page, fills: &[(SlotName, Record)]) -> Result<PageDocument, RenderError> {
    let nodes = template(page).map_err(RenderError::Flaws)?;
    let data = data::page_data(page, fills)?;
    let text = page.content().text();
    let content = match nodes {
        Some(nodes) => {
            walk::worst_case(text, &nodes, &data)?;
            fill(text, data)?
        }
        None => {
            let html = markdown::to_html(text);
            if html.len() > MAX_OUTPUT_BYTES {
                return Err(RenderError::TooLarge {
                    max: MAX_OUTPUT_BYTES,
                });
            }
            format!("<article class=\"prose-board\">\n{html}</article>")
        }
    };
    let mut document = String::with_capacity(content.len() + stylesheet().len() + 256);
    document.push_str(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<style>\n",
    );
    document.push_str(stylesheet());
    document.push_str("</style>\n<title>");
    document.push_str(Html::text(page.name().as_str()).as_str());
    document.push_str("</title>\n</head>\n<body>\n");
    document.push_str(&content);
    document.push_str("\n</body>\n</html>\n");
    Ok(PageDocument(document))
}

/// Serves the page's template as the only source and refuses every partial,
/// so a partial can never reach a file. mustache2's `FsProvider` reads files
/// and its `StaticProvider` leaks each unknown partial name; the template
/// check refuses `{{>` as well.
struct OnePage<'t>(&'t str);

impl SourceProvider for OnePage<'_> {
    type Key = bool;

    fn get_src(&mut self, key: &bool) -> Result<String, Cow<'static, str>> {
        if *key {
            Ok(self.0.to_owned())
        } else {
            Err(Cow::Borrowed("a hangar page has no partials"))
        }
    }

    fn resolve_partial(&self, _key: &bool, _name: &str) -> bool {
        false
    }

    fn display_key(_key: &bool) -> Cow<'_, str> {
        Cow::Borrowed("page")
    }
}

/// Fills a template that passed the check with `data`, once.
fn fill(text: &str, data: Data) -> Result<String, RenderError> {
    let cache = SourceCache::default();
    let mut manager = RenderManager::new(OnePage(text), &cache);
    manager
        .render(true, data)
        .map_err(|error| RenderError::Mustache {
            message: error.message.into_owned(),
        })
}

#[cfg(test)]
mod tests;

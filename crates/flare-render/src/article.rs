//! [`article`]: a Markdown share rendered for the reading page, with its
//! diagrams drawn as SVG, and what the page's script has to do.

use crate::html::Html;
use crate::markdown::{self, ImageSources};
use crate::render::{MAX_OUTPUT_BYTES, RenderError};

/// A Markdown document rendered into the HTML of an `<article>`'s content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    html: Html,
    code: bool,
    diagrams: bool,
}

impl Article {
    /// The rendered HTML. Raw HTML in the Markdown shows as text.
    #[must_use]
    pub fn html(&self) -> &Html {
        &self.html
    }

    /// Whether the article holds a code block, which a script colours.
    #[must_use]
    pub fn has_code(&self) -> bool {
        self.code
    }

    /// Whether the article holds a Mermaid diagram, which the server drew
    /// or showed as its source.
    #[must_use]
    pub fn has_diagrams(&self) -> bool {
        self.diagrams
    }

    /// Whether the page needs its script: it holds code, which the script
    /// colours. A diagram needs no script, because the server drew it.
    #[must_use]
    pub fn needs_script(&self) -> bool {
        self.code
    }
}

/// Renders `markdown` for the reading page. A relative image that
/// `images` lists shows from the URL it gives, and any other image shows
/// as its alternative text.
///
/// A fenced `mermaid` block whose first word is an allowed diagram type
/// becomes a figure holding its drawing as SVG, or its escaped source and a
/// note saying why it is not drawn. Every other code block
/// gets its language label and a Copy button, and a "Figure N." paragraph
/// directly after a diagram, table or code block becomes its caption.
///
/// # Errors
///
/// Returns [`RenderError::TooLarge`] when the HTML is over 4 MiB.
pub fn article(markdown: &str, images: &ImageSources) -> Result<Article, RenderError> {
    let rendered = markdown::render_article(markdown, images);
    if rendered.html.len() > MAX_OUTPUT_BYTES {
        return Err(RenderError::TooLarge {
            max: MAX_OUTPUT_BYTES,
        });
    }
    Ok(Article {
        html: Html::trusted(rendered.html),
        code: rendered.code,
        diagrams: rendered.diagrams,
    })
}

#[cfg(test)]
mod tests;

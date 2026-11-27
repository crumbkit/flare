//! Fenced and indented code blocks: a Mermaid diagram a page may draw
//! becomes a figure, and every other block becomes code with its language
//! label and a Copy button.

use super::Writer;
use super::blocks::diagram_type;
use crate::diagram::{self, Drawing, Kind};
use crate::html::escape_into;
use pulldown_cmark::Event;

/// The label of a code block that names no language.
const PLAIN: &str = "text";

/// A code block being read: its language and its text so far.
#[derive(Debug)]
pub(super) struct CodeBlock {
    pub(super) language: Option<String>,
    pub(super) text: String,
}

impl Writer<'_> {
    /// Writes the code block just read as a diagram or as code.
    pub(super) fn code_end(&mut self) {
        let Some(block) = self.block.take() else {
            return;
        };
        let language = block.language.as_deref().unwrap_or(PLAIN);
        match diagram_type(language, &block.text) {
            Some(kind) => self.diagram(kind, &block.text),
            None => self.listing(language, &block.text),
        }
    }

    /// Writes a diagram's figure. On the reading page the figure holds the
    /// drawing, or the source and a note saying why it is not drawn;
    /// elsewhere it holds the source.
    fn diagram(&mut self, kind: Kind, source: &str) {
        self.diagrams = true;
        if !self.draw {
            self.out.push_str("<figure class=\"diagram\">\n");
            self.source(kind, source);
            self.close_figure();
            return;
        }
        match diagram::draw(kind, source) {
            Drawing::Drawn { svg, min_width } => {
                self.out.push_str(
                    "<figure class=\"diagram\" data-diagram=\"drawn\">\n<div class=\"scroll\" tabindex=\"0\">\n<div class=\"dg\" role=\"img\" aria-label=\"",
                );
                let label = self
                    .caption_text()
                    .unwrap_or_else(|| format!("{} diagram", kind.word()));
                escape_into(&mut self.out, &label);
                // The box takes focus, so a keyboard can scroll a wide
                // diagram, and its minimum width keeps the smallest text
                // at 10.5 px or more.
                self.out.push_str("\" style=\"min-width: ");
                self.out.push_str(&format!("{min_width:.0}"));
                self.out.push_str("px\">");
                self.out.push_str(&svg);
                self.out.push_str("</div>\n</div>\n");
            }
            Drawing::Refused(message) => {
                self.out
                    .push_str("<figure class=\"diagram\" data-diagram=\"failed\">\n");
                self.source(kind, source);
                self.out
                    .push_str("<p class=\"diagram-error\">Flare could not draw this diagram: ");
                escape_into(&mut self.out, &message);
                self.out.push_str("</p>\n");
            }
            Drawing::TooLong => {
                self.out
                    .push_str("<figure class=\"diagram\" data-diagram=\"too-long\">\n");
                self.source(kind, source);
                self.out.push_str(TOO_LONG);
            }
        }
        self.close_figure();
    }

    /// Writes a diagram's escaped source.
    fn source(&mut self, kind: Kind, source: &str) {
        self.out
            .push_str("<pre class=\"diagram-source\" data-diagram=\"");
        self.out.push_str(kind.word());
        self.out.push_str("\">");
        escape_into(&mut self.out, source);
        self.out.push_str("</pre>\n");
    }

    /// Writes a code block with its language label and Copy button.
    fn listing(&mut self, language: &str, text: &str) {
        self.code = true;
        if self.caption.is_some() {
            self.out.push_str("<figure class=\"listing\">\n");
        }
        self.out
            .push_str("<div class=\"code\">\n<div class=\"code-head\"><span class=\"mono\">");
        escape_into(&mut self.out, language);
        self.out.push_str(
            "</span><button class=\"text-btn\" type=\"button\" data-copy hidden>Copy</button></div>\n<pre><code class=\"language-",
        );
        escape_into(&mut self.out, language);
        self.out.push_str("\" data-lang=\"");
        escape_into(&mut self.out, language);
        self.out.push_str("\">");
        escape_into(&mut self.out, text);
        self.out.push_str("</code></pre>\n</div>\n");
        if self.caption.is_some() {
            self.close_figure();
        }
    }

    /// The plain text of the caption of the block being written, if any.
    fn caption_text(&self) -> Option<String> {
        let caption = self.caption.as_ref()?;
        let mut text = String::new();
        for event in caption {
            match event {
                Event::Text(part) | Event::Code(part) => text.push_str(part),
                Event::SoftBreak | Event::HardBreak => text.push(' '),
                _ => {}
            }
        }
        Some(text)
    }

    /// Writes the caption, when the block has one, and closes its figure.
    pub(super) fn close_figure(&mut self) {
        if let Some(caption) = self.caption.take() {
            self.out.push_str("<figcaption>");
            for event in caption {
                self.event(event);
            }
            self.out.push_str("</figcaption>\n");
        }
        self.out.push_str("</figure>\n");
    }
}

/// The note under a diagram whose source is over
/// [`diagram::MAX_SOURCE_BYTES`].
const TOO_LONG: &str = "<p class=\"diagram-error\">This diagram is too long to draw: Flare draws a diagram whose source is at most 50,000 bytes.</p>\n";

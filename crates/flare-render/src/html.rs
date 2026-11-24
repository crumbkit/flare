//! Trusted markup, the escaping that builds it and the stylesheet.

use std::fmt;

/// HTML markup that only this crate builds.
///
/// Every value that reaches it from outside the crate, such as a record
/// field, a label or a message, is escaped first, so a holder of an `Html`
/// may put it in a document as it is. [`Html::text`] is the one public way
/// to build one, and it escapes its text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Html(String);

impl Html {
    /// Escaped text: `&`, `<`, `>`, `"` and `'` become character
    /// references and NUL becomes U+FFFD, so the result is safe in element
    /// content and in a quoted attribute value.
    #[must_use]
    pub fn text(text: &str) -> Self {
        let mut out = String::with_capacity(text.len());
        escape_into(&mut out, text);
        Self(out)
    }

    /// Markup this crate built, with every outside value escaped.
    pub(crate) fn trusted(markup: String) -> Self {
        Self(markup)
    }

    /// The markup.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The markup, as an owned string.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }

    /// Appends other markup.
    pub fn push(&mut self, other: &Html) {
        self.0.push_str(&other.0);
    }
}

impl fmt::Display for Html {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The stylesheet of the page document, the edit form and the conflict view:
/// board-ui's light and dark custom properties, flare's chrome properties
/// and the rules for the class names this crate writes. It holds no `<` and
/// no outside URL.
#[must_use]
pub fn stylesheet() -> &'static str {
    include_str!("../assets/flare.css")
}

/// Appends `text` to `out` with `&`, `<`, `>`, `"` and `'` escaped and NUL
/// replaced by U+FFFD.
pub(crate) fn escape_into(out: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\0' => out.push('\u{FFFD}'),
            other => out.push(other),
        }
    }
}

/// Writes markup element by element. Element and attribute names are the
/// crate's own constants, and every attribute value and text is escaped.
#[derive(Debug, Default)]
pub(crate) struct Builder(String);

impl Builder {
    /// Opens `element` with `attributes`, each written in double quotes.
    pub(crate) fn open(&mut self, element: &'static str, attributes: &[(&'static str, &str)]) {
        self.0.push('<');
        self.0.push_str(element);
        for (name, value) in attributes {
            self.0.push(' ');
            self.0.push_str(name);
            self.0.push_str("=\"");
            escape_into(&mut self.0, value);
            self.0.push('"');
        }
        self.0.push('>');
    }

    /// Writes an attribute that has no value, such as `checked`, into the
    /// element that [`Builder::open`] wrote last. It must follow `open`
    /// directly.
    pub(crate) fn flag(&mut self, name: &'static str) {
        if self.0.ends_with('>') {
            self.0.pop();
            self.0.push(' ');
            self.0.push_str(name);
            self.0.push('>');
        }
    }

    /// Closes `element`.
    pub(crate) fn close(&mut self, element: &'static str) {
        self.0.push_str("</");
        self.0.push_str(element);
        self.0.push('>');
    }

    /// Writes `element` with its attributes and its text, closed.
    pub(crate) fn element(
        &mut self,
        element: &'static str,
        attributes: &[(&'static str, &str)],
        text: &str,
    ) {
        self.open(element, attributes);
        self.text(text);
        self.close(element);
    }

    /// Writes escaped text.
    pub(crate) fn text(&mut self, text: &str) {
        escape_into(&mut self.0, text);
    }

    /// Writes a fixed piece of markup of the crate's own.
    pub(crate) fn raw(&mut self, markup: &'static str) {
        self.0.push_str(markup);
    }

    /// The markup written so far.
    pub(crate) fn finish(self) -> Html {
        Html(self.0)
    }
}

#[cfg(test)]
mod tests;

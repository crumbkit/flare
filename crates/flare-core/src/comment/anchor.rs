//! The place on a Markdown flare's page that a comment is about: the text
//! the reader selected (the quote), up to 64 characters of page text on
//! each side of it, and the version of the flare it was selected on.
//!
//! The server only checks and stores an anchor and returns it with its
//! comment. The reader's browser finds the quote again on each version it
//! shows, so nothing here searches page text.

use crate::error::{ParseError, Subject};
use crate::share::ShareVersion;
use std::fmt;

/// The fewest characters a quote has.
pub const MIN_QUOTE_CHARS: usize = 3;

/// The most characters a quote has.
pub const MAX_QUOTE_CHARS: usize = 500;

/// The most characters of page text kept on each side of a quote.
pub const MAX_CONTEXT_CHARS: usize = 64;

/// Checks that `text` holds no control character. Page text has each run
/// of whitespace collapsed to one space, so a quote and its context never
/// hold a newline or a tab.
fn check_plain(subject: Subject, text: &str) -> Result<(), ParseError> {
    if text.chars().any(char::is_control) {
        return Err(ParseError::Malformed {
            subject,
            rule: "text without control characters",
        });
    }
    Ok(())
}

/// The text a reader selected on the page: [`MIN_QUOTE_CHARS`] to
/// [`MAX_QUOTE_CHARS`] characters with no control character. It quotes
/// the page, so its `Debug` output gives only its length.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Quote(String);

impl Quote {
    /// The quote as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for Quote {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        let subject = Subject::AnchorQuote;
        if text.is_empty() {
            return Err(ParseError::Empty { subject });
        }
        let length = text.chars().count();
        if length > MAX_QUOTE_CHARS {
            return Err(ParseError::TooLong {
                subject,
                max: MAX_QUOTE_CHARS,
            });
        }
        if length < MIN_QUOTE_CHARS {
            return Err(ParseError::Malformed {
                subject,
                rule: "3 to 500 characters",
            });
        }
        check_plain(subject, text)?;
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Debug for Quote {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Quote(<{} chars>)", self.0.chars().count())
    }
}

/// The page text on one side of a quote: 0 to [`MAX_CONTEXT_CHARS`]
/// characters with no control character. Its `Debug` output gives only its
/// length.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct QuoteContext(String);

impl QuoteContext {
    /// The context as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for QuoteContext {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        let subject = Subject::AnchorContext;
        if text.chars().count() > MAX_CONTEXT_CHARS {
            return Err(ParseError::TooLong {
                subject,
                max: MAX_CONTEXT_CHARS,
            });
        }
        check_plain(subject, text)?;
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Debug for QuoteContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "QuoteContext(<{} chars>)",
            self.0.chars().count()
        )
    }
}

/// Where on a Markdown flare a comment is: the quote, the page text before
/// and after it, and the version it was selected on. An anchor never
/// changes once written, and a delete erases it with the comment's text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Anchor {
    quote: Quote,
    prefix: QuoteContext,
    suffix: QuoteContext,
    version: ShareVersion,
}

impl Anchor {
    /// The anchor of `quote` with the text before it, `prefix`, and after
    /// it, `suffix`, on `version`.
    #[must_use]
    pub fn new(
        quote: Quote,
        prefix: QuoteContext,
        suffix: QuoteContext,
        version: ShareVersion,
    ) -> Self {
        Self {
            quote,
            prefix,
            suffix,
            version,
        }
    }

    /// The selected text.
    #[must_use]
    pub fn quote(&self) -> &Quote {
        &self.quote
    }

    /// The page text just before the quote.
    #[must_use]
    pub fn prefix(&self) -> &QuoteContext {
        &self.prefix
    }

    /// The page text just after the quote.
    #[must_use]
    pub fn suffix(&self) -> &QuoteContext {
        &self.suffix
    }

    /// The version the quote was selected on.
    #[must_use]
    pub fn version(&self) -> ShareVersion {
        self.version
    }
}

#[cfg(test)]
mod tests;

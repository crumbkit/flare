//! Shares: a document that one owner publishes in numbered versions and
//! opens to the people a grant names.
//!
//! This module holds the validated values. [`records`] holds what a store
//! keeps and returns.

mod html;
mod listing;
mod records;

pub use html::{HtmlFile, MAX_HTML};

pub use listing::Listing;
pub use records::{NewShare, NewVersion, Share, SharedVersion};

use crate::error::{ParseError, Subject};
use std::fmt;
use std::num::NonZeroU32;

/// The most versions one share has. A publish to a share that has this many
/// answers [`ShareStoreError::TooManyVersions`](crate::ShareStoreError) and
/// writes nothing.
pub const MAX_VERSIONS: u32 = 100;

/// The most shares one owner has. A create by an owner who has this many
/// answers [`ShareStoreError::TooManyShares`](crate::ShareStoreError) and
/// writes nothing. It equals [`MAX_LISTED`], so an owner's list is never
/// truncated.
pub const MAX_SHARES_PER_OWNER: usize = 500;

/// The most entries one [`Listing`] holds. A list with more entries holds
/// the first this many and says it is truncated.
pub const MAX_LISTED: usize = 500;

/// The longest title, in bytes.
pub const MAX_TITLE: usize = 200;

/// The longest document, in bytes: 256 KiB, so that one version fits one
/// item of a store whose items hold at most 400 KB.
pub const MAX_DOCUMENT: usize = 262_144;

/// The bytes of a random id.
const ID_BYTES: usize = 16;

/// Writes 16 bytes as 32 lower-case hexadecimal characters.
pub(crate) fn hex_id(bytes: [u8; ID_BYTES]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(ID_BYTES * 2);
    for byte in bytes {
        for nibble in [byte >> 4, byte & 0x0f] {
            let digit = DIGITS.get(usize::from(nibble)).copied().unwrap_or(b'0');
            text.push(char::from(digit));
        }
    }
    text
}

/// Checks that `text` is 32 lower-case hexadecimal characters.
pub(crate) fn check_hex_id(subject: Subject, text: &str) -> Result<(), ParseError> {
    let hex = text
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if text.len() == ID_BYTES * 2 && hex {
        Ok(())
    } else {
        Err(ParseError::Malformed {
            subject,
            rule: "32 lower-case hexadecimal characters",
        })
    }
}

/// The id of a share: 128 random bits as 32 lower-case hexadecimal
/// characters. The caller draws the bits, so this crate needs no source of
/// randomness. It is the only part of a link that names a share.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShareId(String);

impl ShareId {
    /// The id that 16 random bytes give.
    #[must_use]
    pub fn from_random(bytes: [u8; 16]) -> Self {
        Self(hex_id(bytes))
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for ShareId {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        check_hex_id(Subject::ShareId, text)?;
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Display for ShareId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The title of a share: 1 to 200 bytes with no control character. A page
/// escapes it when it shows it. A title may name a person, so its `Debug`
/// output gives only its length.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ShareTitle(String);

impl ShareTitle {
    /// The title as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for ShareTitle {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        let subject = Subject::ShareTitle;
        if text.is_empty() {
            return Err(ParseError::Empty { subject });
        }
        if text.len() > MAX_TITLE {
            return Err(ParseError::TooLong {
                subject,
                max: MAX_TITLE,
            });
        }
        if text.chars().any(char::is_control) {
            return Err(ParseError::Malformed {
                subject,
                rule: "text without control characters",
            });
        }
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Debug for ShareTitle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ShareTitle(<{} bytes>)", self.0.len())
    }
}

/// What kind of content a share holds. A match on it outside this crate
/// needs a wildcard arm, so a later kind does not break callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ShareKind {
    /// A Markdown document.
    Document,
    /// An uploaded HTML file, shown in a sandboxed frame.
    Html,
}

/// The number of one version of a share. Versions of a share run from 1 to
/// its latest with no gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShareVersion(NonZeroU32);

impl ShareVersion {
    /// Version 1, which a create writes.
    #[must_use]
    pub const fn first() -> Self {
        Self(NonZeroU32::MIN)
    }

    /// The version after this one, or `None` past `u32::MAX`.
    #[must_use]
    pub fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }

    /// The number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl TryFrom<u32> for ShareVersion {
    type Error = ParseError;

    fn try_from(number: u32) -> Result<Self, ParseError> {
        NonZeroU32::new(number)
            .map(Self)
            .ok_or(ParseError::Malformed {
                subject: Subject::ShareVersion,
                rule: "a number from 1",
            })
    }
}

/// The Markdown of a document: 1 to 262,144 bytes with no NUL. Markdown may
/// hold personal data, so its `Debug` output gives only its length.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DocumentText(String);

impl DocumentText {
    /// The Markdown.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for DocumentText {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        let subject = Subject::DocumentText;
        if text.is_empty() {
            return Err(ParseError::Empty { subject });
        }
        if text.len() > MAX_DOCUMENT {
            return Err(ParseError::TooLong {
                subject,
                max: MAX_DOCUMENT,
            });
        }
        if text.contains('\0') {
            return Err(ParseError::Malformed {
                subject,
                rule: "text without NUL",
            });
        }
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Debug for DocumentText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "DocumentText(<{} bytes>)", self.0.len())
    }
}

/// The content of one version of a share. A match on it outside this crate
/// needs a wildcard arm, so a later kind does not break callers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ShareContent {
    /// A Markdown document.
    Document(DocumentText),
    /// An uploaded HTML file, held in the file store.
    Html(HtmlFile),
}

impl ShareContent {
    /// The kind of share this content belongs to.
    #[must_use]
    pub fn kind(&self) -> ShareKind {
        match self {
            Self::Document(_) => ShareKind::Document,
            Self::Html(_) => ShareKind::Html,
        }
    }
}

/// Prints `<principal>` in place of a principal id in `Debug` output, so a
/// log line never holds an address.
pub(crate) struct HiddenPrincipal;

impl fmt::Debug for HiddenPrincipal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<principal>")
    }
}

#[cfg(test)]
mod tests;

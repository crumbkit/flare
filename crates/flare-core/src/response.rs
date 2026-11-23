//! Response links: a share that names a flare with `--re` responds to it,
//! and each link has a position in one increasing sequence that the
//! activity cursor counts.

use crate::name::PrincipalId;
use crate::share::{ShareId, ShareVersion};
use crate::time::Timestamp;

/// The most links one activity page holds.
pub const MAX_PAGE: usize = 100;

/// A place in the one increasing sequence of response links. Position 0
/// comes before every link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Position(u64);

impl Position {
    /// The position `value`.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The position as a number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A link to write: `response` responds to version `version` of `flare`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NewLink {
    /// The responding share.
    pub response: ShareId,
    /// The flare it responds to.
    pub flare: ShareId,
    /// The flare's version when the response named it.
    pub version: ShareVersion,
    /// When the link was made.
    pub at: Timestamp,
}

impl NewLink {
    /// A link from `response` to version `version` of `flare`, made at `at`.
    #[must_use]
    pub fn new(response: ShareId, flare: ShareId, version: ShareVersion, at: Timestamp) -> Self {
        Self {
            response,
            flare,
            version,
            at,
        }
    }

    /// The stored link at `position`.
    #[must_use]
    pub fn at_position(self, position: Position) -> ResponseLink {
        ResponseLink {
            position,
            response: self.response,
            flare: self.flare,
            version: self.version,
            at: self.at,
        }
    }
}

/// A stored link with its position.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResponseLink {
    /// The link's place in the sequence.
    pub position: Position,
    /// The responding share.
    pub response: ShareId,
    /// The flare it responds to.
    pub flare: ShareId,
    /// The flare's version when the response named it.
    pub version: ShareVersion,
    /// When the link was made.
    pub at: Timestamp,
}

/// Whom an activity index entry lists a response to.
///
/// Each reader has its own index, so a read of one reader's index holds
/// only the responses that reader can open. A group that shares a flare
/// would be a second variant with its own index.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Reader {
    /// One person, by the principal id that grants and owners name.
    Person(PrincipalId),
}

/// An activity index entry to write: `reader` can open both the response
/// and the flare of `link`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NewEntry {
    /// Whom the entry lists the response to.
    pub reader: Reader,
    /// The link the entry lists.
    pub link: ResponseLink,
}

impl NewEntry {
    /// An entry that lists `link` to `reader`.
    #[must_use]
    pub fn new(reader: Reader, link: ResponseLink) -> Self {
        Self { reader, link }
    }

    /// The stored entry at `position`.
    #[must_use]
    pub fn at_position(self, position: Position) -> IndexEntry {
        IndexEntry {
            reader: self.reader,
            position,
            response: self.link.response,
            flare: self.link.flare,
            version: self.link.version,
            at: self.link.at,
        }
    }
}

/// A stored activity index entry. Its position is its own, taken from the
/// one sequence that links also take positions from, so an entry written
/// after a reader's cursor is new to that reader whatever its link's age.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct IndexEntry {
    /// Whom the entry lists the response to.
    pub reader: Reader,
    /// The entry's place in the sequence.
    pub position: Position,
    /// The responding share.
    pub response: ShareId,
    /// The flare it responds to.
    pub flare: ShareId,
    /// The flare's version when the response named it.
    pub version: ShareVersion,
    /// When the response named the flare.
    pub at: Timestamp,
}

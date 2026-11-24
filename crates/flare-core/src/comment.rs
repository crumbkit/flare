//! Comments on a flare and replies to a comment, and the named cursors
//! that remember how far a person has read.
//!
//! A comment is one record of a flare. A reply is a comment with a
//! `parent`, and a reply has no replies of its own. Every create, edit and
//! delete is an event with a position in the one sequence that response
//! links share, so the activity feed reads comments and responses in one
//! order.

use crate::error::{ParseError, Subject};
use crate::name::PrincipalId;
use crate::response::Position;
use crate::share::{ShareId, check_hex_id, hex_id};
use crate::time::Timestamp;
use std::fmt;

/// The most characters one comment body has.
pub const MAX_COMMENT_CHARS: usize = 8_000;

/// The most comments and replies, together, that one flare holds. One
/// read of a flare's comments returns all of them.
pub const MAX_COMMENTS_PER_FLARE: usize = 2_000;

/// The most named cursors one person has.
pub const MAX_CURSORS_PER_PERSON: usize = 32;

/// The longest comment key or cursor name, in characters.
const MAX_NAME: usize = 64;

/// The id of a comment: 128 random bits as 32 lower-case hexadecimal
/// characters, drawn by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommentId(String);

impl CommentId {
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

impl TryFrom<&str> for CommentId {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        check_hex_id(Subject::CommentId, text)?;
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Display for CommentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The text of a comment: 1 to [`MAX_COMMENT_CHARS`] characters, with no
/// control character other than a newline, a carriage return or a tab. A
/// page escapes it when it shows it. A body is a person's words, so its
/// `Debug` output gives only its length.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CommentBody(String);

impl CommentBody {
    /// The body as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for CommentBody {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        let subject = Subject::CommentBody;
        if text.is_empty() {
            return Err(ParseError::Empty { subject });
        }
        if text.chars().count() > MAX_COMMENT_CHARS {
            return Err(ParseError::TooLong {
                subject,
                max: MAX_COMMENT_CHARS,
            });
        }
        if text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err(ParseError::Malformed {
                subject,
                rule: "text without control characters other than newline and tab",
            });
        }
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Debug for CommentBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "CommentBody(<{} chars>)", self.0.chars().count())
    }
}

/// Checks a key or a name: 1 to 64 characters of letters, digits, `.`, `_`
/// and `-`.
fn check_name(subject: Subject, text: &str) -> Result<(), ParseError> {
    if text.is_empty() {
        return Err(ParseError::Empty { subject });
    }
    if text.len() > MAX_NAME {
        return Err(ParseError::TooLong {
            subject,
            max: MAX_NAME,
        });
    }
    if text
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        Ok(())
    } else {
        Err(ParseError::Malformed {
            subject,
            rule: "letters, digits, dots, underscores and hyphens",
        })
    }
}

/// The key that a client draws for one comment it sends, so that sending
/// it again after a lost answer does not write a second comment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CommentKey(String);

impl CommentKey {
    /// The key as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for CommentKey {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        check_name(Subject::CommentKey, text)?;
        Ok(Self(text.to_owned()))
    }
}

/// The name of a cursor: 1 to 64 characters of letters, digits, `.`, `_`
/// and `-`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CursorName(String);

impl CursorName {
    /// The name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for CursorName {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        check_name(Subject::CursorName, text)?;
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Display for CursorName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A comment to write.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NewComment {
    /// The server-drawn id.
    pub id: CommentId,
    /// The flare it is on.
    pub flare: ShareId,
    /// The comment it replies to, or `None` for a comment on the flare.
    pub parent: Option<CommentId>,
    /// Who wrote it.
    pub author: PrincipalId,
    /// The text.
    pub body: CommentBody,
    /// The client's key for this comment.
    pub key: CommentKey,
    /// When it was written.
    pub at: Timestamp,
}

impl NewComment {
    /// A comment with the server-drawn `id`.
    #[must_use]
    pub fn new(
        id: CommentId,
        flare: ShareId,
        parent: Option<CommentId>,
        author: PrincipalId,
        body: CommentBody,
        key: CommentKey,
        at: Timestamp,
    ) -> Self {
        Self {
            id,
            flare,
            parent,
            author,
            body,
            key,
            at,
        }
    }
}

/// A stored comment or reply.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Comment {
    /// The id.
    pub id: CommentId,
    /// The flare it is on.
    pub flare: ShareId,
    /// The comment it replies to, or `None` for a comment on the flare.
    pub parent: Option<CommentId>,
    /// Who wrote it.
    pub author: PrincipalId,
    /// The text, or `None` once the author deleted it.
    pub body: Option<CommentBody>,
    /// When it was written.
    pub created: Timestamp,
    /// When its text last changed, or `None` for a text never edited.
    pub edited: Option<Timestamp>,
    /// The position of its creation in the sequence, which also orders the
    /// comments of a flare.
    pub position: Position,
}

impl Comment {
    /// A stored comment that a store read back, with no edit.
    #[must_use]
    pub fn new(
        id: CommentId,
        flare: ShareId,
        parent: Option<CommentId>,
        author: PrincipalId,
        body: Option<CommentBody>,
        created: Timestamp,
        position: Position,
    ) -> Self {
        Self {
            id,
            flare,
            parent,
            author,
            body,
            created,
            edited: None,
            position,
        }
    }

    /// The same comment with `edited` as its edit time.
    #[must_use]
    pub fn with_edited(mut self, edited: Option<Timestamp>) -> Self {
        self.edited = edited;
        self
    }

    /// The stored comment that `new` becomes at `position`.
    #[must_use]
    pub fn from_new(new: NewComment, position: Position) -> Self {
        Self {
            id: new.id,
            flare: new.flare,
            parent: new.parent,
            author: new.author,
            body: Some(new.body),
            created: new.at,
            edited: None,
            position,
        }
    }

    /// Whether the author deleted it.
    #[must_use]
    pub fn is_deleted(&self) -> bool {
        self.body.is_none()
    }
}

/// What an event of a comment did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CommentAction {
    /// The comment or reply was written.
    Created,
    /// Its text changed.
    Edited,
    /// Its author deleted it.
    Deleted,
}

/// One create, edit or delete of a comment, at its own position.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommentEvent {
    /// The event's place in the sequence.
    pub position: Position,
    /// What happened.
    pub action: CommentAction,
    /// The comment it happened to.
    pub comment: CommentId,
    /// The flare the comment is on.
    pub flare: ShareId,
    /// The comment's parent, or `None` for a comment on the flare.
    pub parent: Option<CommentId>,
    /// The comment's author, who made every event of it.
    pub author: PrincipalId,
    /// When it happened.
    pub at: Timestamp,
}

impl CommentEvent {
    /// An event that a store read back.
    #[must_use]
    pub fn new(
        position: Position,
        action: CommentAction,
        comment: CommentId,
        flare: ShareId,
        parent: Option<CommentId>,
        author: PrincipalId,
        at: Timestamp,
    ) -> Self {
        Self {
            position,
            action,
            comment,
            flare,
            parent,
            author,
            at,
        }
    }

    /// The event of `action` on `comment`, at `position`, at the moment
    /// `at`.
    #[must_use]
    pub fn of(comment: &Comment, action: CommentAction, position: Position, at: Timestamp) -> Self {
        Self::new(
            position,
            action,
            comment.id.clone(),
            comment.flare.clone(),
            comment.parent.clone(),
            comment.author.clone(),
            at,
        )
    }
}

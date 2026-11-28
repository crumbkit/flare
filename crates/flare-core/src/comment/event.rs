//! The events of a comment: each create, edit and delete at its own
//! position in the sequence.

use super::{Comment, CommentId};
use crate::name::PrincipalId;
use crate::response::Position;
use crate::share::ShareId;
use crate::time::Timestamp;

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

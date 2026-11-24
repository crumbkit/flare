//! The [`CommentStore`] and [`CursorStore`] traits and their error.

use crate::comment::{Comment, CommentBody, CommentEvent, CommentId, CursorName, NewComment};
use crate::name::PrincipalId;
use crate::response::Position;
use crate::share::ShareId;
use crate::time::Timestamp;
use std::future::Future;

/// Why a comment or cursor store refused an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CommentStoreError {
    /// No comment has that id.
    #[error("no such comment")]
    NotFound,
    /// The author deleted the comment, so its text cannot change.
    #[error("the comment was deleted")]
    Deleted,
    /// The flare already holds the most comments and replies it may, or
    /// the person already holds the most cursors they may. Nothing was
    /// written.
    #[error("the limit is reached")]
    TooMany,
    /// The store could not answer. After a write the outcome is unknown.
    #[error("the comment store is unavailable")]
    Unavailable,
}

/// What [`CommentStore::add`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Added {
    /// The comment was written.
    Created(Comment),
    /// The author had already sent a comment on this flare with this key,
    /// so nothing was written and this is the first comment.
    Repeated(Comment),
}

/// Keeps the comments and replies of flares.
///
/// Each create, edit and delete takes one position from the sequence that
/// the [`ResponseStore`](crate::ResponseStore) shares, and its event is
/// visible to a read exactly when its position is at or below the
/// response store's head. The store holds at most
/// [`MAX_COMMENTS_PER_FLARE`](crate::MAX_COMMENTS_PER_FLARE) comments and
/// replies per flare.
pub trait CommentStore: Send + Sync {
    /// Writes a comment, once per author, flare and key. A repeat answers
    /// the first comment as [`Added::Repeated`], before the flare's limit
    /// is checked.
    ///
    /// # Errors
    ///
    /// Returns [`CommentStoreError::TooMany`] and
    /// [`CommentStoreError::Unavailable`].
    fn add(
        &self,
        comment: NewComment,
    ) -> impl Future<Output = Result<Added, CommentStoreError>> + Send;

    /// Reads one comment.
    ///
    /// # Errors
    ///
    /// Returns [`CommentStoreError::NotFound`] and
    /// [`CommentStoreError::Unavailable`].
    fn get(
        &self,
        id: &CommentId,
    ) -> impl Future<Output = Result<Comment, CommentStoreError>> + Send;

    /// Every comment and reply of `flare`, deleted ones included, oldest
    /// first.
    ///
    /// # Errors
    ///
    /// Returns [`CommentStoreError::Unavailable`].
    fn of_flare(
        &self,
        flare: &ShareId,
    ) -> impl Future<Output = Result<Vec<Comment>, CommentStoreError>> + Send;

    /// Replaces the text of a comment and writes an edit event. The author
    /// check belongs to the caller.
    ///
    /// # Errors
    ///
    /// Returns [`CommentStoreError::NotFound`],
    /// [`CommentStoreError::Deleted`] and
    /// [`CommentStoreError::Unavailable`].
    fn edit(
        &self,
        id: &CommentId,
        body: CommentBody,
        at: Timestamp,
    ) -> impl Future<Output = Result<Comment, CommentStoreError>> + Send;

    /// Erases the text of a comment, keeps the rest and writes a delete
    /// event. Deleting a deleted comment answers it unchanged and writes
    /// nothing.
    ///
    /// # Errors
    ///
    /// Returns [`CommentStoreError::NotFound`] and
    /// [`CommentStoreError::Unavailable`].
    fn delete(
        &self,
        id: &CommentId,
        at: Timestamp,
    ) -> impl Future<Output = Result<Comment, CommentStoreError>> + Send;

    /// The events on `flare` whose position is above `after` and at most
    /// `upto`, oldest first, at most `limit` of them.
    ///
    /// # Errors
    ///
    /// Returns [`CommentStoreError::Unavailable`].
    fn events(
        &self,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<CommentEvent>, CommentStoreError>> + Send;
}

/// Keeps each person's named cursors, which remember a position of the
/// activity sequence.
pub trait CursorStore: Send + Sync {
    /// The position of the person's cursor, or `None` when they have none
    /// of that name.
    ///
    /// # Errors
    ///
    /// Returns [`CommentStoreError::Unavailable`].
    fn get(
        &self,
        owner: &PrincipalId,
        name: &CursorName,
    ) -> impl Future<Output = Result<Option<Position>, CommentStoreError>> + Send;

    /// Moves the cursor to `to` when `to` is above its position, makes it
    /// when it does not exist, and answers the position it holds after.
    /// A cursor never moves back.
    ///
    /// # Errors
    ///
    /// Returns [`CommentStoreError::TooMany`] for a new cursor past
    /// [`MAX_CURSORS_PER_PERSON`](crate::MAX_CURSORS_PER_PERSON), and
    /// [`CommentStoreError::Unavailable`].
    fn advance(
        &self,
        owner: &PrincipalId,
        name: &CursorName,
        to: Position,
    ) -> impl Future<Output = Result<Position, CommentStoreError>> + Send;
}

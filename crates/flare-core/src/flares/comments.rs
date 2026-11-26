//! Comments, replies and cursors through [`Flares`].
//!
//! A person who can open a flare may read its comments and write comments
//! and replies on it. Only a comment's author edits or deletes it, and only
//! while they can still open the flare. A reply belongs to a comment of the
//! same flare that is not itself a reply. A flare that a caller cannot open
//! and a comment that is not there give the same answer.

use super::{FlareError, Flares, flare_refused};
use crate::PrincipalId;
use crate::auth::Principal;
use crate::comment::{Comment, CommentBody, CommentId, CommentKey, CursorName, NewComment};
use crate::response::Position;
use crate::share::ShareId;
use crate::store::{
    Added, CommentStore, CommentStoreError, CursorStore, GrantStore, ResponseStore, ShareStore,
};
use crate::teams::Teams;
use crate::time::Timestamp;

/// A comment that a caller asks to write.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommentDraft {
    id: CommentId,
    flare: ShareId,
    parent: Option<CommentId>,
    body: CommentBody,
    key: CommentKey,
}

impl CommentDraft {
    /// A comment on `flare`, or a reply to `parent`, with the
    /// server-drawn `id` and the client's `key`.
    #[must_use]
    pub fn new(
        id: CommentId,
        flare: ShareId,
        parent: Option<CommentId>,
        body: CommentBody,
        key: CommentKey,
    ) -> Self {
        Self {
            id,
            flare,
            parent,
            body,
            key,
        }
    }
}

/// A comment with its replies, oldest first.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommentThread {
    /// The comment.
    pub comment: Comment,
    /// The replies to it.
    pub replies: Vec<Comment>,
}

/// The answer to writing a comment.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Posted {
    /// The comment written now, or the first one that the same key wrote.
    pub comment: Comment,
    /// Whether this call wrote it.
    pub created: bool,
}

fn comment_refused(error: CommentStoreError) -> FlareError {
    match error {
        CommentStoreError::NotFound | CommentStoreError::Deleted => FlareError::CommentNotFound,
        CommentStoreError::TooMany => FlareError::TooManyComments,
        _ => FlareError::CommentsUnavailable,
    }
}

impl<S: ShareStore, G: GrantStore, R: ResponseStore, C: CommentStore, U: CursorStore, T: Teams, K>
    Flares<S, G, R, C, U, T, K>
{
    /// Checks that `who` can open `flare` and answers who they are.
    async fn readable(&self, who: &Principal, flare: &ShareId) -> Result<PrincipalId, FlareError> {
        let person = self.person_id(who)?;
        self.shares
            .open(who, flare, None)
            .await
            .map(|_| person)
            .map_err(flare_refused)
    }

    /// The comment `id`, when `who` can still open its flare.
    async fn visible(
        &self,
        who: &Principal,
        id: &CommentId,
    ) -> Result<(Comment, PrincipalId), FlareError> {
        let comment = self.comments.get(id).await.map_err(comment_refused)?;
        match self.readable(who, &comment.flare).await {
            Ok(person) => Ok((comment, person)),
            Err(FlareError::NotFound) => Err(FlareError::CommentNotFound),
            Err(other) => Err(other),
        }
    }

    /// The comments of `flare` with their replies, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`FlareError::NotFound`] when the caller cannot open the
    /// flare, [`FlareError::Share`] and [`FlareError::CommentsUnavailable`].
    pub async fn comments(
        &self,
        who: &Principal,
        flare: &ShareId,
    ) -> Result<Vec<CommentThread>, FlareError> {
        self.readable(who, flare).await?;
        let all = self
            .comments
            .of_flare(flare)
            .await
            .map_err(comment_refused)?;
        let mut threads: Vec<CommentThread> = Vec::new();
        for comment in all.iter().filter(|comment| comment.parent.is_none()) {
            let replies = all
                .iter()
                .filter(|reply| reply.parent.as_ref() == Some(&comment.id))
                .cloned()
                .collect();
            threads.push(CommentThread {
                comment: comment.clone(),
                replies,
            });
        }
        Ok(threads)
    }

    /// Writes a comment on a flare, or a reply to one of its comments, as
    /// the caller. Sending one key twice answers the first comment.
    ///
    /// # Errors
    ///
    /// Returns [`FlareError::NotFound`] when the caller cannot open the
    /// flare, [`FlareError::CommentNotFound`] when the parent is not a
    /// comment of the flare, [`FlareError::ReplyToReply`] when it is a
    /// reply, [`FlareError::TooManyComments`] and
    /// [`FlareError::CommentsUnavailable`].
    pub async fn post_comment(
        &self,
        who: &Principal,
        draft: CommentDraft,
        at: Timestamp,
    ) -> Result<Posted, FlareError> {
        let author = self.readable(who, &draft.flare).await?;
        if let Some(parent) = &draft.parent {
            let held = self.comments.get(parent).await.map_err(comment_refused)?;
            if held.flare != draft.flare {
                return Err(FlareError::CommentNotFound);
            }
            if held.parent.is_some() {
                return Err(FlareError::ReplyToReply);
            }
        }
        let new = NewComment::new(
            draft.id,
            draft.flare,
            draft.parent,
            author,
            draft.body,
            draft.key,
            at,
        );
        match self.comments.add(new).await.map_err(comment_refused)? {
            Added::Created(comment) => Ok(Posted {
                comment,
                created: true,
            }),
            Added::Repeated(comment) => Ok(Posted {
                comment,
                created: false,
            }),
        }
    }

    /// Replaces the text of the caller's own comment.
    ///
    /// # Errors
    ///
    /// Returns [`FlareError::CommentNotFound`] for an unknown or deleted
    /// comment and for a flare the caller can no longer open,
    /// [`FlareError::NotCommentAuthor`] and
    /// [`FlareError::CommentsUnavailable`].
    pub async fn edit_comment(
        &self,
        who: &Principal,
        id: &CommentId,
        body: CommentBody,
        at: Timestamp,
    ) -> Result<Comment, FlareError> {
        let (held, person) = self.visible(who, id).await?;
        if held.author != person {
            return Err(FlareError::NotCommentAuthor);
        }
        self.comments
            .edit(id, body, at)
            .await
            .map_err(comment_refused)
    }

    /// Erases the text of the caller's own comment and keeps its replies.
    ///
    /// # Errors
    ///
    /// Returns [`FlareError::CommentNotFound`],
    /// [`FlareError::NotCommentAuthor`] and
    /// [`FlareError::CommentsUnavailable`].
    pub async fn delete_comment(
        &self,
        who: &Principal,
        id: &CommentId,
        at: Timestamp,
    ) -> Result<Comment, FlareError> {
        let (held, person) = self.visible(who, id).await?;
        if held.author != person {
            return Err(FlareError::NotCommentAuthor);
        }
        self.comments.delete(id, at).await.map_err(comment_refused)
    }

    /// The position of the caller's cursor `name`.
    ///
    /// # Errors
    ///
    /// Returns [`FlareError::CursorNotFound`] and
    /// [`FlareError::CommentsUnavailable`].
    pub async fn cursor(&self, who: &Principal, name: &CursorName) -> Result<Position, FlareError> {
        self.cursors
            .get(&self.person_id(who)?, name)
            .await
            .map_err(comment_refused)?
            .ok_or(FlareError::CursorNotFound)
    }

    /// Moves the caller's cursor `name` to `to` when that is further than
    /// it is, and answers where it is after.
    ///
    /// # Errors
    ///
    /// Returns [`FlareError::TooManyCursors`] and
    /// [`FlareError::CommentsUnavailable`].
    pub async fn advance_cursor(
        &self,
        who: &Principal,
        name: &CursorName,
        to: Position,
    ) -> Result<Position, FlareError> {
        self.cursors
            .advance(&self.person_id(who)?, name, to)
            .await
            .map_err(|error| match error {
                CommentStoreError::TooMany => FlareError::TooManyCursors,
                other => comment_refused(other),
            })
    }
}

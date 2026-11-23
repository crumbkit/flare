//! Why [`Flares`](super::Flares) refused a request.

use super::MAX_FLARES;
use crate::ShareId;
use crate::sharing::ShareError;

/// Why [`Flares`] refused a request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FlareError {
    /// The caller is not on the creators list.
    #[error("only the deployment's creators may share")]
    NotACreator,
    /// A named flare does not exist, or the caller cannot open it. The two
    /// give the same answer.
    #[error("no such flare")]
    NotFound,
    /// The request names more than [`MAX_FLARES`] flares.
    #[error("a request names at most {MAX_FLARES} flares")]
    TooManyFlares,
    /// The share exists, and a grant or a response link after it failed.
    /// `granted` grants were written; the owner finishes with a grant.
    #[error("the share {share} was created, and only {granted} of its grants were written")]
    PartlyShared {
        /// The share that exists.
        share: ShareId,
        /// How many grants were written.
        granted: usize,
        /// Why the next write failed.
        cause: ShareError,
    },
    /// The share service refused.
    #[error(transparent)]
    Share(ShareError),
    /// The response store could not answer.
    #[error("the response store is unavailable")]
    ResponsesUnavailable,
    /// No such comment, or the caller cannot open its flare, or the
    /// author deleted it.
    #[error("no such comment")]
    CommentNotFound,
    /// Only a comment's author edits or deletes it.
    #[error("only the author edits or deletes a comment")]
    NotCommentAuthor,
    /// A reply names a reply as its parent, and a reply has no replies.
    #[error("a reply has no replies")]
    ReplyToReply,
    /// The flare holds the most comments and replies it may.
    #[error("the flare holds the most comments it may")]
    TooManyComments,
    /// The caller has no cursor of that name.
    #[error("no such cursor")]
    CursorNotFound,
    /// The caller holds the most cursors they may.
    #[error("the caller holds the most cursors they may")]
    TooManyCursors,
    /// The comment or cursor store could not answer.
    #[error("the comment store is unavailable")]
    CommentsUnavailable,
}

//! The [`ResponseStore`] trait and its error.

use crate::response::{NewLink, Position, ResponseLink};
use crate::share::ShareId;
use std::future::Future;

/// Why a response store could not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ResponseStoreError {
    /// The store could not answer. After a write the outcome is unknown.
    #[error("the response store is unavailable")]
    Unavailable,
}

/// Keeps the links from a response to the flares it responds to.
///
/// Every link gets a position larger than every position given before it,
/// and a link is visible to a read as soon as its position is the head, so
/// a reader that reads the head and then the links up to it misses no link
/// at or below that head.
pub trait ResponseStore: Send + Sync {
    /// Writes a link, once per response and flare. A repeat answers the
    /// first link unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`ResponseStoreError::Unavailable`].
    fn link(
        &self,
        link: NewLink,
    ) -> impl Future<Output = Result<ResponseLink, ResponseStoreError>> + Send;

    /// The highest position given, or position 0 when there is no link.
    ///
    /// # Errors
    ///
    /// Returns [`ResponseStoreError::Unavailable`].
    fn head(&self) -> impl Future<Output = Result<Position, ResponseStoreError>> + Send;

    /// The links to `flare` whose position is above `after` and at most
    /// `upto`, oldest first, at most `limit` of them.
    ///
    /// # Errors
    ///
    /// Returns [`ResponseStoreError::Unavailable`].
    fn to_flare(
        &self,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<ResponseLink>, ResponseStoreError>> + Send;

    /// Every link from `response`, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`ResponseStoreError::Unavailable`].
    fn of_response(
        &self,
        response: &ShareId,
    ) -> impl Future<Output = Result<Vec<ResponseLink>, ResponseStoreError>> + Send;
}

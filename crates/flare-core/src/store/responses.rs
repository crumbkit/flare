//! The [`ResponseStore`] trait and its error.

use crate::response::{IndexEntry, NewEntry, NewLink, Position, Reader, ResponseLink};
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

/// Keeps the links from a response to the flares it responds to, and each
/// reader's activity index of the links that reader can open.
///
/// Every link and every index entry gets a position larger than every
/// position given before it, from one sequence, and is visible to a read as
/// soon as its position is the head, so a reader that reads the head and
/// then the links or entries up to it misses none at or below that head.
///
/// The store keeps whatever entries it is given: the caller decides who can
/// open what, and a reader of the index opens each response again.
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

    /// The links from each of `responses` to `flare`, in the order of
    /// `responses`, skipping a response with no such link.
    ///
    /// # Errors
    ///
    /// Returns [`ResponseStoreError::Unavailable`].
    fn pairs(
        &self,
        responses: &[ShareId],
        flare: &ShareId,
    ) -> impl Future<Output = Result<Vec<ResponseLink>, ResponseStoreError>> + Send;

    /// Writes each entry, once per reader, response and flare. Each new
    /// entry takes a new position; an entry already written keeps its
    /// first position and is not written again.
    ///
    /// # Errors
    ///
    /// Returns [`ResponseStoreError::Unavailable`]. Entries written before
    /// the failure stay, and a repeat writes only the missing ones.
    fn index(
        &self,
        entries: Vec<NewEntry>,
    ) -> impl Future<Output = Result<(), ResponseStoreError>> + Send;

    /// Removes every entry of `reader` for `response`, on every flare.
    /// Removing what is absent changes nothing.
    ///
    /// # Errors
    ///
    /// Returns [`ResponseStoreError::Unavailable`].
    fn unindex(
        &self,
        reader: &Reader,
        response: &ShareId,
    ) -> impl Future<Output = Result<(), ResponseStoreError>> + Send;

    /// The entries of `reader` on `flare` whose position is above `after`
    /// and at most `upto`, oldest first, at most `limit` of them.
    ///
    /// # Errors
    ///
    /// Returns [`ResponseStoreError::Unavailable`].
    fn visible(
        &self,
        reader: &Reader,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<IndexEntry>, ResponseStoreError>> + Send;
}

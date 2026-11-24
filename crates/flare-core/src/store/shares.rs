//! The [`ShareStore`] trait and its error.

use crate::name::PrincipalId;
use crate::share::{Listing, NewShare, NewVersion, Share, ShareId, ShareVersion, SharedVersion};
use std::future::Future;

/// Why a share store refused an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ShareStoreError {
    /// The share, or the version of it, does not exist.
    #[error("no such share or version")]
    NotFound,
    /// A share with that id exists. The first share is unchanged.
    #[error("a share with that id exists")]
    IdTaken,
    /// The owner already has [`MAX_SHARES_PER_OWNER`](crate::MAX_SHARES_PER_OWNER)
    /// shares. Nothing was written.
    #[error("the owner has as many shares as one owner may have")]
    TooManyShares,
    /// The share already has [`MAX_VERSIONS`](crate::MAX_VERSIONS) versions.
    /// Nothing was written.
    #[error("the share has as many versions as one share may have")]
    TooManyVersions,
    /// The store could not answer. After a write the outcome is unknown: the
    /// write may have been applied.
    #[error("the share store is unavailable")]
    Unavailable,
}

/// Keeps shares and their versions.
///
/// Every read returns every write that completed before it started,
/// including the owner's list. A version is never changed or removed, and
/// the versions of one share run from 1 to its latest with no gap.
pub trait ShareStore: Send + Sync {
    /// Writes a new share and its version 1 together, or nothing.
    ///
    /// # Errors
    ///
    /// Returns [`ShareStoreError::IdTaken`] when the id is taken, and leaves
    /// the first share unchanged; [`ShareStoreError::TooManyShares`] when the
    /// owner has [`MAX_SHARES_PER_OWNER`](crate::MAX_SHARES_PER_OWNER)
    /// shares; and [`ShareStoreError::Unavailable`] when the store cannot
    /// answer.
    fn create(&self, new: NewShare) -> impl Future<Output = Result<Share, ShareStoreError>> + Send;

    /// Reads a share.
    ///
    /// # Errors
    ///
    /// Returns [`ShareStoreError::NotFound`] for an unknown share and
    /// [`ShareStoreError::Unavailable`] when the store cannot answer.
    fn get(&self, id: &ShareId) -> impl Future<Output = Result<Share, ShareStoreError>> + Send;

    /// Adds the next version to a share and moves its latest to it. Two
    /// publishes that run together get two consecutive numbers.
    ///
    /// # Errors
    ///
    /// Returns [`ShareStoreError::NotFound`] for an unknown share,
    /// [`ShareStoreError::TooManyVersions`] when the share has
    /// [`MAX_VERSIONS`](crate::MAX_VERSIONS) versions, and
    /// [`ShareStoreError::Unavailable`] when the store cannot answer. A
    /// definite error writes nothing and uses up no number.
    fn publish(
        &self,
        id: &ShareId,
        version: NewVersion,
    ) -> impl Future<Output = Result<ShareVersion, ShareStoreError>> + Send;

    /// Reads one version of a share.
    ///
    /// # Errors
    ///
    /// Returns [`ShareStoreError::NotFound`] for an unknown share and for a
    /// number above the share's latest, and [`ShareStoreError::Unavailable`]
    /// when the store cannot answer.
    fn version(
        &self,
        id: &ShareId,
        number: ShareVersion,
    ) -> impl Future<Output = Result<SharedVersion, ShareStoreError>> + Send;

    /// The shares that `owner` owns, newest first by creation time and then
    /// by id, at most [`MAX_LISTED`](crate::MAX_LISTED) of them.
    ///
    /// # Errors
    ///
    /// Returns [`ShareStoreError::Unavailable`] when the store cannot answer.
    fn owned_by(
        &self,
        owner: &PrincipalId,
    ) -> impl Future<Output = Result<Listing<Share>, ShareStoreError>> + Send;
}

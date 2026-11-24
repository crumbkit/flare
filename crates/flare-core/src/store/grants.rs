//! The [`GrantStore`] trait and its error.

use super::ListKey;
use crate::grant::{Grant, GrantRevoke, GrantWrite, Role};
use crate::name::PrincipalId;
use crate::share::{Listing, ShareId};
use std::future::Future;

/// Why a grant store refused a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum GrantStoreError {
    /// The grantee's grant is not the one the writer saw. Nothing was
    /// written, and the writer reads again before it decides.
    #[error("the grant changed since it was read")]
    Changed {
        /// The grantee's role now, or `None` when the grantee holds no grant.
        current: Option<Role>,
    },
    /// The write acts under an admin who no longer holds an Admin grant on
    /// the share. Nothing was written.
    #[error("the admin the write acts under no longer holds the Admin grant")]
    StaleAuthority,
    /// The share has [`MAX_GRANTS`](crate::MAX_GRANTS) grants and the
    /// grantee holds none. Nothing was written.
    #[error("the share has as many grants as one share may have")]
    TooMany,
    /// The store could not answer. After a write the outcome is unknown: the
    /// write may have been applied.
    #[error("the grant store is unavailable")]
    Unavailable,
}

/// Keeps the grants of shares.
///
/// Every read returns every write that completed before it started,
/// including both lists, so a revoke is visible to the next read. A store
/// holds no shares: a grant on a share it holds nothing for is applied, and
/// no method answers that a share does not exist.
///
/// A write applies only when the grant item is unchanged since the store
/// read it, so a revoke and a re-grant of the same role between the
/// store's read and its write make it read again. When its own write is
/// cancelled by a concurrent writer, a store reads again and retries at
/// least three times before it answers
/// [`GrantStoreError::Unavailable`]; with two writers running together it
/// never gives up.
pub trait GrantStore: Send + Sync {
    /// Gives the grantee the role, when their current role equals the
    /// write's expected role. A role change keeps the grant's time of first
    /// grant. A write under [`Granter::Admin`](crate::Granter::Admin)
    /// applies only while that person holds an Admin grant on the share.
    ///
    /// When more than one error applies, the store answers the first of
    /// [`GrantStoreError::Unavailable`], [`GrantStoreError::StaleAuthority`],
    /// [`GrantStoreError::Changed`] and [`GrantStoreError::TooMany`].
    ///
    /// # Errors
    ///
    /// Returns [`GrantStoreError::StaleAuthority`] when the acting admin no
    /// longer holds the Admin grant, [`GrantStoreError::Changed`] with the
    /// current role when it differs from the expected one,
    /// [`GrantStoreError::TooMany`] for a new grantee on a share with
    /// [`MAX_GRANTS`](crate::MAX_GRANTS) grants and
    /// [`GrantStoreError::Unavailable`] when the store cannot answer.
    fn grant(
        &self,
        write: GrantWrite,
    ) -> impl Future<Output = Result<Grant, GrantStoreError>> + Send;

    /// Ends the grantee's grant, when its role equals the expected role. A
    /// revoke under [`Revoker::Admin`](crate::Revoker::Admin) applies only
    /// while that person holds an Admin grant on the share, and one under
    /// [`Revoker::Grantee`](crate::Revoker::Grantee) checks the expected
    /// role alone.
    ///
    /// # Errors
    ///
    /// The same as [`GrantStore::grant`], in the same order, except
    /// [`GrantStoreError::TooMany`]. A revoke of a person with no grant
    /// answers `Changed { current: None }`.
    fn revoke(
        &self,
        revoke: GrantRevoke,
    ) -> impl Future<Output = Result<(), GrantStoreError>> + Send;

    /// The grantee's grant on the share, or `None`.
    ///
    /// # Errors
    ///
    /// Returns [`GrantStoreError::Unavailable`] when the store cannot answer.
    fn get(
        &self,
        share: &ShareId,
        grantee: &PrincipalId,
    ) -> impl Future<Output = Result<Option<Grant>, GrantStoreError>> + Send;

    /// Every grant of one share, ordered by time of first grant and then by
    /// grantee. A share has at most [`MAX_GRANTS`](crate::MAX_GRANTS).
    ///
    /// # Errors
    ///
    /// Returns [`GrantStoreError::Unavailable`] when the store cannot answer.
    fn of_share(
        &self,
        share: &ShareId,
    ) -> impl Future<Output = Result<Vec<Grant>, GrantStoreError>> + Send;

    /// The grantee's grants across shares that sort after `after`, or from
    /// the first without it, newest first by time of first grant and then
    /// by share: at most `limit` of them, with the truncation flag set when
    /// more follow. A `limit` above [`MAX_LISTED`](crate::MAX_LISTED) reads
    /// [`MAX_LISTED`](crate::MAX_LISTED), and a `limit` of 0 reads one.
    ///
    /// `after` need not name a grant the store holds, so a key whose grant
    /// was revoked since starts at the grant after it. A grant written
    /// during a walk of pages makes no grant that was there repeat or go
    /// missing; it is read when it sorts after the key, and not otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`GrantStoreError::Unavailable`] when the store cannot answer.
    fn granted_page(
        &self,
        grantee: &PrincipalId,
        after: Option<&ListKey>,
        limit: usize,
    ) -> impl Future<Output = Result<Listing<Grant>, GrantStoreError>> + Send;

    /// The grantee's first [`MAX_LISTED`](crate::MAX_LISTED) grants across
    /// shares, as [`GrantStore::granted_page`] reads them with no key.
    ///
    /// # Errors
    ///
    /// Returns [`GrantStoreError::Unavailable`] when the store cannot answer.
    fn granted_to(
        &self,
        grantee: &PrincipalId,
    ) -> impl Future<Output = Result<Listing<Grant>, GrantStoreError>> + Send {
        self.granted_page(grantee, None, crate::MAX_LISTED)
    }
}

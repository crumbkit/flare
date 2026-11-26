//! The [`TokenStore`] trait and its error.

use crate::person::Person;
use crate::time::Timestamp;
use crate::token::{Digest, LoginCode, NewToken, StoredToken, TokenId, UsedCode};
use std::future::Future;

/// Why a token store refused an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TokenStoreError {
    /// No token has that id, or it belongs to someone else.
    #[error("no such token")]
    NotFound,
    /// The login code is unknown, used or expired. The three give the same
    /// answer.
    #[error("the login code is unknown, used or expired")]
    CodeRefused,
    /// A code with that digest, or a token with that id, exists. Nothing
    /// was written; the caller draws a new value.
    #[error("that code or token id is taken")]
    Taken,
    /// The store could not answer. After a write the outcome is unknown.
    #[error("the token store is unavailable")]
    Unavailable,
}

/// Keeps login codes and CLI tokens, by digest, never their raw values.
///
/// Every read returns every write that completed before it started.
pub trait TokenStore: Send + Sync {
    /// Stores a new login code.
    ///
    /// # Errors
    ///
    /// Returns [`TokenStoreError::Taken`] when a code with that digest
    /// exists and [`TokenStoreError::Unavailable`].
    fn issue_code(
        &self,
        code: LoginCode,
    ) -> impl Future<Output = Result<(), TokenStoreError>> + Send;

    /// Marks the code with `digest` used, once. Two uses that run together
    /// give one success.
    ///
    /// # Errors
    ///
    /// Returns [`TokenStoreError::CodeRefused`] for an unknown, used or
    /// expired code, and [`TokenStoreError::Unavailable`].
    fn use_code(
        &self,
        digest: &Digest,
        now: Timestamp,
    ) -> impl Future<Output = Result<UsedCode, TokenStoreError>> + Send;

    /// Stores a new token, and revokes, at the new token's creation time,
    /// the owner's active token with the same label. Answers the id of the
    /// token it revoked.
    ///
    /// # Errors
    ///
    /// Returns [`TokenStoreError::Taken`] when the id is taken and
    /// [`TokenStoreError::Unavailable`].
    fn create(
        &self,
        token: NewToken,
    ) -> impl Future<Output = Result<Option<TokenId>, TokenStoreError>> + Send;

    /// Reads a token, revoked or not.
    ///
    /// # Errors
    ///
    /// Returns [`TokenStoreError::NotFound`] and
    /// [`TokenStoreError::Unavailable`].
    fn get(
        &self,
        id: &TokenId,
    ) -> impl Future<Output = Result<StoredToken, TokenStoreError>> + Send;

    /// Revokes a token of `owner` at `at`. A token revoked before keeps its
    /// first revoked time.
    ///
    /// # Errors
    ///
    /// Returns [`TokenStoreError::NotFound`] for an unknown token and for
    /// another owner's token, and [`TokenStoreError::Unavailable`].
    fn revoke(
        &self,
        id: &TokenId,
        owner: &Person,
        at: Timestamp,
    ) -> impl Future<Output = Result<(), TokenStoreError>> + Send;
}

//! [`ShareError`], the answer to a refused request about a share.

use super::Refusal;
use crate::store::{FileStoreError, GrantStoreError, ShareStoreError};

/// Why the [`Shares`](super::Shares) service refused a request. No variant
/// carries a value, so no answer holds an address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ShareError {
    /// The share does not exist, or the caller has no standing in it. The
    /// two give the same answer, so a caller cannot tell them apart.
    #[error("no such share")]
    NotFound,
    /// The caller may read the share, and it has no version with that
    /// number.
    #[error("the share has no such version")]
    VersionNotFound,
    /// A revoke names a person who holds no grant on the share.
    #[error("that person holds no grant on the share")]
    GrantNotFound,
    /// The caller has a standing in the share and lacks the right.
    #[error("the caller may not do that to the share")]
    Forbidden,
    /// The request would grant to, revoke or otherwise change the owner.
    #[error("the owner of a share cannot be changed")]
    OwnerCannotBeChanged,
    /// The target's grant changed between the read and the write. Nothing
    /// was written; the caller reads again.
    #[error("the grant changed since it was read")]
    GrantChanged,
    /// The share has as many grants as one share may have.
    #[error("the share has as many grants as one share may have")]
    TooManyGrants,
    /// The share has as many versions as one share may have.
    #[error("the share has as many versions as one share may have")]
    TooManyVersions,
    /// The caller owns as many shares as one owner may have.
    #[error("the caller owns as many shares as one owner may have")]
    TooManyShares,
    /// A share with that id exists. The caller draws a new id.
    #[error("a share with that id exists")]
    IdTaken,
    /// The caller's sign-in verified no email address, and shares are
    /// decided by it.
    #[error("the caller has no verified email address")]
    NoVerifiedEmail,
    /// A store could not answer. After a write the outcome is unknown.
    #[error("the share stores are unavailable")]
    Unavailable,
}

impl From<Refusal> for ShareError {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::NotFound => Self::NotFound,
            Refusal::Forbidden => Self::Forbidden,
            Refusal::OwnerCannotBeChanged => Self::OwnerCannotBeChanged,
            Refusal::GrantNotFound => Self::GrantNotFound,
        }
    }
}

impl From<ShareStoreError> for ShareError {
    /// A store's answer. A missing share is the caller's `NotFound`; every
    /// other failure is a limit error or `Unavailable`, never access.
    fn from(error: ShareStoreError) -> Self {
        match error {
            ShareStoreError::NotFound => Self::NotFound,
            ShareStoreError::IdTaken => Self::IdTaken,
            ShareStoreError::TooManyShares => Self::TooManyShares,
            ShareStoreError::TooManyVersions => Self::TooManyVersions,
            ShareStoreError::Unavailable => Self::Unavailable,
        }
    }
}

impl From<GrantStoreError> for ShareError {
    /// A grant store's refusal of a write: a changed grant, an admin whose
    /// authority ended, the limit, or `Unavailable`.
    fn from(error: GrantStoreError) -> Self {
        match error {
            GrantStoreError::Changed { .. } => Self::GrantChanged,
            GrantStoreError::StaleAuthority => Self::Forbidden,
            GrantStoreError::TooMany => Self::TooManyGrants,
            GrantStoreError::Unavailable => Self::Unavailable,
        }
    }
}

impl From<FileStoreError> for ShareError {
    /// A file store's answer to a read. A missing file is `NotFound`.
    fn from(error: FileStoreError) -> Self {
        match error {
            FileStoreError::NotFound => Self::NotFound,
            FileStoreError::IdTaken | FileStoreError::Unavailable => Self::Unavailable,
        }
    }
}

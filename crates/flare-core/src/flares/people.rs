//! Who may create a share, and how a `--with` identifier names a person.

use crate::auth::Principal;
use crate::email::Email;
use std::future::Future;

/// Who may create a share, responses included.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Creators {
    /// Everyone the sign-in admits, by their verified address.
    Everyone,
    /// Only these verified addresses.
    Only(Vec<Email>),
}

impl Creators {
    /// Only the people with these addresses.
    #[must_use]
    pub fn only(addresses: Vec<Email>) -> Self {
        Self::Only(addresses)
    }

    /// Whether `who` may create a share. A principal with no verified
    /// address may not.
    #[must_use]
    pub fn admits(&self, who: &Principal) -> bool {
        match (self, who.email()) {
            (_, None) => false,
            (Self::Everyone, Some(_)) => true,
            (Self::Only(addresses), Some(email)) => addresses.contains(email),
        }
    }
}

/// Why a `--with` identifier names nobody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RecipientError {
    /// The identifier is not an email address, and the deployment resolves
    /// only addresses.
    #[error("the recipient is not an email address")]
    NotAnAddress,
    /// The deployment's directory could not answer.
    #[error("the recipients directory is unavailable")]
    Unavailable,
}

/// Turns a `--with` identifier into the verified address that grants bind
/// to. A deployment supplies its own, such as one that looks a name up in
/// its directory.
pub trait Recipients: Send + Sync {
    /// The address that `identifier` names.
    ///
    /// # Errors
    ///
    /// Returns [`RecipientError::NotAnAddress`] for an identifier the
    /// deployment cannot resolve and [`RecipientError::Unavailable`].
    fn resolve(
        &self,
        identifier: &str,
    ) -> impl Future<Output = Result<Email, RecipientError>> + Send;
}

/// The default [`Recipients`]: an identifier must be an email address.
///
/// Shortcut: names, user ids and `team:` recipients are refused. A
/// deployment that needs them supplies its own `Recipients`.
#[derive(Debug, Clone, Copy, Default)]
pub struct AddressesOnly;

impl Recipients for AddressesOnly {
    async fn resolve(&self, identifier: &str) -> Result<Email, RecipientError> {
        Email::try_from(identifier).map_err(|_| RecipientError::NotAnAddress)
    }
}

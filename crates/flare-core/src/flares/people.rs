//! Who may create a share, and how a `--with` identifier names a person.

use crate::auth::Principal;
use crate::email::Email;
use crate::error::{ParseError, Subject};
use std::collections::HashMap;
use std::future::Future;

/// A whole email domain, such as `example.com`, written `@example.com` in
/// a creators list and held in lower case.
///
/// Shortcut: only an exact domain is accepted. Wildcards and subdomains
/// are refused, so `@example.com` does not admit `ana@mail.example.com`.
/// A domain is a run of visible ASCII characters without a second `@` or
/// a `*`, and is not checked against DNS syntax, because the provider has
/// verified the address and flare only compares its domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatorDomain(String);

impl TryFrom<&str> for CreatorDomain {
    type Error = ParseError;

    /// Admits `@` and a domain, such as `@example.com`.
    fn try_from(text: &str) -> Result<Self, ParseError> {
        let malformed = ParseError::Malformed {
            subject: Subject::Email,
            rule: "a domain entry is an @ and a domain, such as @example.com, without a * or a second @",
        };
        let Some(domain) = text.strip_prefix('@') else {
            return Err(malformed);
        };
        let clean = domain
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'@' && byte != b'*');
        if domain.is_empty() || domain.len() > 253 || !clean {
            return Err(malformed);
        }
        Ok(Self(domain.to_ascii_lowercase()))
    }
}

impl CreatorDomain {
    /// Whether `email` is at exactly this domain.
    fn holds(&self, email: &Email) -> bool {
        email
            .as_str()
            .rsplit_once('@')
            .is_some_and(|(_, domain)| domain == self.0)
    }
}

/// Who may create a share, responses included.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Creators {
    /// Everyone the sign-in admits, by their verified address.
    Everyone,
    /// Only these verified addresses and everyone at these domains.
    Only {
        /// The addresses.
        addresses: Vec<Email>,
        /// The domains.
        domains: Vec<CreatorDomain>,
    },
}

impl Creators {
    /// Only the people with these addresses.
    #[must_use]
    pub fn only(addresses: Vec<Email>) -> Self {
        Self::listed(addresses, Vec::new())
    }

    /// The people with these addresses and everyone at these domains.
    #[must_use]
    pub fn listed(addresses: Vec<Email>, domains: Vec<CreatorDomain>) -> Self {
        Self::Only { addresses, domains }
    }

    /// Whether `who` may create a share. A principal with no verified
    /// address may not.
    #[must_use]
    pub fn admits(&self, who: &Principal) -> bool {
        match (self, who.email()) {
            (_, None) => false,
            (Self::Everyone, Some(_)) => true,
            (Self::Only { addresses, domains }, Some(email)) => {
                addresses.contains(email) || domains.iter().any(|domain| domain.holds(email))
            }
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

/// [`Recipients`] for a deployment that maps usernames to addresses: an
/// identifier is a configured name, else an email address.
///
/// Shortcut: the map is fixed when the server starts. Names are matched
/// in lower case, and user ids, `team:` recipients and a directory lookup
/// are not supported. An empty map resolves addresses only, like
/// [`AddressesOnly`].
#[derive(Debug, Clone, Default)]
pub struct NamedRecipients {
    names: HashMap<String, Email>,
}

impl NamedRecipients {
    /// Recipients that resolve each of these names to its address. A name
    /// is held in lower case.
    #[must_use]
    pub fn new(names: impl IntoIterator<Item = (String, Email)>) -> Self {
        Self {
            names: names
                .into_iter()
                .map(|(name, email)| (name.to_ascii_lowercase(), email))
                .collect(),
        }
    }
}

impl Recipients for NamedRecipients {
    async fn resolve(&self, identifier: &str) -> Result<Email, RecipientError> {
        if let Some(email) = self.names.get(&identifier.to_ascii_lowercase()) {
            return Ok(email.clone());
        }
        Email::try_from(identifier).map_err(|_| RecipientError::NotAnAddress)
    }
}

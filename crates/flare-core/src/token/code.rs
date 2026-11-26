//! Login codes: the one-use code that a signed-in browser hands to the
//! CLI, and what it gave once it was used.

use super::{CODE_LIFE, Digest};
use crate::email::Email;
use crate::person::Person;
use crate::share::HiddenPrincipal;
use crate::time::Timestamp;
use std::fmt;

/// A one-use login code that a signed-in browser hands to the CLI.
#[derive(Clone, PartialEq, Eq)]
pub struct LoginCode {
    digest: Digest,
    owner: Person,
    email: Option<Email>,
    port: u16,
    expires: Timestamp,
}

impl LoginCode {
    /// A code of `owner` for the CLI listening on `port`, issued at
    /// `issued` and expiring [`CODE_LIFE`] seconds later.
    #[must_use]
    pub fn new(digest: Digest, owner: Person, port: u16, issued: Timestamp) -> Self {
        Self {
            digest,
            owner,
            email: None,
            port,
            expires: Timestamp::from_secs(issued.as_secs().saturating_add(CODE_LIFE)),
        }
    }

    /// The same code, keeping `email`, the address the owner's sign-in
    /// verified, beside an owner named by username. An owner named by an
    /// address keeps only that address, so `email` is then ignored.
    #[must_use]
    pub fn with_email(mut self, email: Option<Email>) -> Self {
        self.email = email.filter(|_| self.owner.as_email().is_none());
        self
    }

    /// The digest of the code.
    #[must_use]
    pub fn digest(&self) -> &Digest {
        &self.digest
    }

    /// The person the code signs in.
    #[must_use]
    pub fn owner(&self) -> &Person {
        &self.owner
    }

    /// The email address kept beside the owner, as [`StoredToken::email`](crate::StoredToken::email)
    /// answers it.
    #[must_use]
    pub fn email(&self) -> Option<&Email> {
        self.email.as_ref().or_else(|| self.owner.as_email())
    }

    /// The CLI's loopback port.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// When the code stops working.
    #[must_use]
    pub fn expires(&self) -> Timestamp {
        self.expires
    }

    /// Whether the code is before its expiry at `now`.
    #[must_use]
    pub fn is_live(&self, now: Timestamp) -> bool {
        now < self.expires
    }

    /// What a successful use of the code answers.
    #[must_use]
    pub fn used(&self) -> UsedCode {
        UsedCode {
            owner: self.owner.clone(),
            email: self.email().cloned(),
            port: self.port,
        }
    }
}

impl fmt::Debug for LoginCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoginCode")
            .field("owner", &HiddenPrincipal)
            .field("port", &self.port)
            .field("expires", &self.expires)
            .finish_non_exhaustive()
    }
}

/// What a login code gave once it was used: the person it signs in and the
/// port it was issued for.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct UsedCode {
    /// The person the code signs in.
    pub owner: Person,
    /// The email address the owner's sign-in verified, if any.
    pub email: Option<Email>,
    /// The CLI's loopback port the code was issued for.
    pub port: u16,
}

impl UsedCode {
    /// What a used code of `owner`, issued for `port`, gave. An owner
    /// named by an address has that address as its email.
    #[must_use]
    pub fn new(owner: Person, port: u16) -> Self {
        let email = owner.as_email().cloned();
        Self { owner, email, port }
    }

    /// The same answer, with `email` kept beside an owner named by
    /// username.
    #[must_use]
    pub fn with_email(mut self, email: Option<Email>) -> Self {
        if self.owner.as_email().is_none() {
            self.email = email;
        }
        self
    }
}

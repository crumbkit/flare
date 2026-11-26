//! CLI tokens and the one-use login codes that buy them.
//!
//! A store keeps only the SHA-256 [`Digest`] of a token's secret or of a
//! login code, never the raw value. The caller computes the digest, so this
//! crate needs no hash function. The lifecycle of both is in
//! `docs/tokens.md`.

use crate::email::Email;
use crate::error::{ParseError, Subject};
use crate::person::Person;
use crate::share::HiddenPrincipal;
use crate::share::{check_hex_id, hex_id};
use crate::time::Timestamp;
use std::fmt;

mod digest;

pub use digest::Digest;

/// How long a token lives after it is created: 90 days, in seconds.
pub const TOKEN_LIFE: u64 = 90 * 24 * 60 * 60;

/// How long a login code lives after it is issued: 60 seconds.
pub const CODE_LIFE: u64 = 60;

/// The longest token label, in bytes.
const MAX_LABEL: usize = 64;

/// The id of a token: 128 random bits as 32 lower-case hexadecimal
/// characters. The token's text starts with it, so a store finds the token
/// by its id and compares the digest of the rest.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenId(String);

impl TokenId {
    /// The id that 16 random bytes give.
    #[must_use]
    pub fn from_random(bytes: [u8; 16]) -> Self {
        Self(hex_id(bytes))
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for TokenId {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        check_hex_id(Subject::TokenId, text)?;
        Ok(Self(text.to_owned()))
    }
}

/// What a person calls a token, such as the name of the Mac it lives on:
/// 1 to 64 bytes of visible ASCII and spaces. One owner holds at most one
/// active token per label.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenLabel(String);

impl TokenLabel {
    /// The label as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for TokenLabel {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        let subject = Subject::TokenLabel;
        if text.is_empty() {
            return Err(ParseError::Empty { subject });
        }
        if text.len() > MAX_LABEL {
            return Err(ParseError::TooLong {
                subject,
                max: MAX_LABEL,
            });
        }
        if !text
            .bytes()
            .all(|byte| byte == b' ' || byte.is_ascii_graphic())
        {
            return Err(ParseError::Malformed {
                subject,
                rule: "visible ASCII characters and spaces",
            });
        }
        Ok(Self(text.to_owned()))
    }
}

/// A token as a store keeps it.
#[derive(Clone, PartialEq, Eq)]
pub struct StoredToken {
    id: TokenId,
    digest: Digest,
    owner: Person,
    email: Option<Email>,
    label: TokenLabel,
    created: Timestamp,
    expires: Timestamp,
    revoked: Option<Timestamp>,
}

impl StoredToken {
    /// A token read back from a store.
    #[must_use]
    pub fn new(new: NewToken, expires: Timestamp, revoked: Option<Timestamp>) -> Self {
        Self {
            id: new.id,
            digest: new.digest,
            owner: new.owner,
            email: new.email,
            label: new.label,
            created: new.created,
            expires,
            revoked,
        }
    }

    /// The token's id.
    #[must_use]
    pub fn id(&self) -> &TokenId {
        &self.id
    }

    /// The digest of the token's secret.
    #[must_use]
    pub fn digest(&self) -> &Digest {
        &self.digest
    }

    /// The person the token authenticates as.
    #[must_use]
    pub fn owner(&self) -> &Person {
        &self.owner
    }

    /// The email address the owner's sign-in verified when the token was
    /// made: the owner's own address under the email kind, and the address
    /// kept beside a username, if there was one.
    #[must_use]
    pub fn email(&self) -> Option<&Email> {
        self.email.as_ref().or_else(|| self.owner.as_email())
    }

    /// The token's label.
    #[must_use]
    pub fn label(&self) -> &TokenLabel {
        &self.label
    }

    /// When the token was created.
    #[must_use]
    pub fn created(&self) -> Timestamp {
        self.created
    }

    /// When the token stops working: [`TOKEN_LIFE`] after its creation.
    #[must_use]
    pub fn expires(&self) -> Timestamp {
        self.expires
    }

    /// When the token was revoked, if it was.
    #[must_use]
    pub fn revoked(&self) -> Option<Timestamp> {
        self.revoked
    }

    /// The same token, revoked at `at` unless it was revoked before.
    #[must_use]
    pub fn revoked_at(mut self, at: Timestamp) -> Self {
        self.revoked = Some(self.revoked.unwrap_or(at));
        self
    }

    /// Whether the token authenticates at `now`: not revoked, and before
    /// its expiry.
    #[must_use]
    pub fn is_active(&self, now: Timestamp) -> bool {
        self.revoked.is_none() && now < self.expires
    }
}

impl fmt::Debug for StoredToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StoredToken")
            .field("id", &self.id)
            .field("owner", &HiddenPrincipal)
            .field("created", &self.created)
            .field("expires", &self.expires)
            .field("revoked", &self.revoked)
            .finish_non_exhaustive()
    }
}

/// A token to store.
#[derive(Clone, PartialEq, Eq)]
pub struct NewToken {
    id: TokenId,
    digest: Digest,
    owner: Person,
    email: Option<Email>,
    label: TokenLabel,
    created: Timestamp,
}

impl NewToken {
    /// A token of `owner`, created at `created`, whose secret has the
    /// digest `digest`.
    #[must_use]
    pub fn new(
        id: TokenId,
        digest: Digest,
        owner: Person,
        label: TokenLabel,
        created: Timestamp,
    ) -> Self {
        Self {
            id,
            digest,
            owner,
            email: None,
            label,
            created,
        }
    }

    /// The same token, keeping `email`, the address the owner's sign-in
    /// verified, beside an owner named by username. An owner named by an
    /// address keeps only that address, so `email` is then ignored.
    #[must_use]
    pub fn with_email(mut self, email: Option<Email>) -> Self {
        self.email = email.filter(|_| self.owner.as_email().is_none());
        self
    }

    /// The token's id.
    #[must_use]
    pub fn id(&self) -> &TokenId {
        &self.id
    }

    /// The token's owner.
    #[must_use]
    pub fn owner(&self) -> &Person {
        &self.owner
    }

    /// The email address kept beside the owner, as [`StoredToken::email`]
    /// answers it.
    #[must_use]
    pub fn email(&self) -> Option<&Email> {
        self.email.as_ref().or_else(|| self.owner.as_email())
    }

    /// The token's label.
    #[must_use]
    pub fn label(&self) -> &TokenLabel {
        &self.label
    }

    /// When the token was created.
    #[must_use]
    pub fn created(&self) -> Timestamp {
        self.created
    }

    /// The token as a store keeps it once written: active, expiring
    /// [`TOKEN_LIFE`] after its creation.
    #[must_use]
    pub fn stored(self) -> StoredToken {
        let expires = Timestamp::from_secs(self.created.as_secs().saturating_add(TOKEN_LIFE));
        StoredToken::new(self, expires, None)
    }
}

impl fmt::Debug for NewToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewToken")
            .field("id", &self.id)
            .field("owner", &HiddenPrincipal)
            .field("created", &self.created)
            .finish_non_exhaustive()
    }
}

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

    /// The email address kept beside the owner, as [`StoredToken::email`]
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

#[cfg(test)]
mod tests;

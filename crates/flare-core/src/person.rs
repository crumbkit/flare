//! [`Person`], the identifier a deployment names people by: an [`Email`]
//! or a [`Username`], of the one [`IdentityKind`] the deployment chose.

use crate::email::Email;
use crate::error::{ParseError, Subject};
use crate::name::PrincipalId;
use std::fmt;

/// The longest username, in bytes.
const MAX_USERNAME: usize = 64;

/// The words no username may be: `everyone` is the everyone grantee, and
/// `me` is the caller in a search's `from:`.
const RESERVED: [&str; 2] = ["everyone", "me"];

/// A directory username: 1 to 64 bytes of lower-case ASCII letters, digits,
/// dots, underscores and dashes that starts with a letter or a digit, held
/// in lower case, and never `everyone` or `me`.
///
/// A username holds no `@`, so it is never read as an address, and no `:`,
/// so it is never read as a `team:` grantee.
///
/// Its `Debug` output hides the name.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Username(PrincipalId);

impl TryFrom<&str> for Username {
    type Error = ParseError;

    /// Admits one username and keeps it in lower case.
    fn try_from(text: &str) -> Result<Self, ParseError> {
        let subject = Subject::Username;
        if text.is_empty() {
            return Err(ParseError::Empty { subject });
        }
        if text.len() > MAX_USERNAME {
            return Err(ParseError::TooLong {
                subject,
                max: MAX_USERNAME,
            });
        }
        let malformed = ParseError::Malformed {
            subject,
            rule: "lower-case letters, digits, dots, underscores and dashes, starting with a letter or a digit, and not everyone or me",
        };
        let lower = text.to_ascii_lowercase();
        let allowed = |byte: u8| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        };
        let starts = lower
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
        if !starts || !lower.bytes().all(allowed) || RESERVED.contains(&lower.as_str()) {
            return Err(malformed);
        }
        // A username of at most 64 visible ASCII bytes is always a valid
        // principal id, whose rule is 1 to 256 of them.
        PrincipalId::try_from(lower.as_str())
            .map(Self)
            .map_err(|_| malformed)
    }
}

impl Username {
    /// The username in lower case.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for Username {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Username(<hidden>)")
    }
}

/// How a deployment names people. A deployment uses one kind for every
/// grant, team member and token, and never mixes the two.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum IdentityKind {
    /// People are named by their verified email address. This is the
    /// default.
    #[default]
    Email,
    /// People are named by their directory username.
    Username,
}

impl IdentityKind {
    /// The kind as config and a store's marker write it: `email` or
    /// `username`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Email => "email",
            Self::Username => "username",
        }
    }

    /// The person `text` names under this kind.
    ///
    /// # Errors
    ///
    /// Returns the [`Email`] or [`Username`] parser's error for text that
    /// is not a value of this kind.
    pub fn person(self, text: &str) -> Result<Person, ParseError> {
        match self {
            Self::Email => Email::try_from(text).map(Person::Email),
            Self::Username => Username::try_from(text).map(Person::Username),
        }
    }
}

impl TryFrom<&str> for IdentityKind {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        match text {
            "email" => Ok(Self::Email),
            "username" => Ok(Self::Username),
            _ => Err(ParseError::Malformed {
                subject: Subject::IdentityKind,
                rule: "email or username",
            }),
        }
    }
}

/// One person as a deployment names them: by address or by username.
///
/// A stored id with an `@` is always an address and one without is always
/// a username, so [`Person::try_from`] reads back what
/// [`Person::principal_id`] wrote.
///
/// Its `Debug` output hides the value.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Person {
    /// A person named by their verified address.
    Email(Email),
    /// A person named by their directory username.
    Username(Username),
}

impl TryFrom<&str> for Person {
    type Error = ParseError;

    /// Reads an address when `text` holds an `@` and a username otherwise.
    fn try_from(text: &str) -> Result<Self, ParseError> {
        if text.contains('@') {
            IdentityKind::Email.person(text)
        } else {
            IdentityKind::Username.person(text)
        }
    }
}

impl From<Email> for Person {
    fn from(email: Email) -> Self {
        Self::Email(email)
    }
}

impl From<Username> for Person {
    fn from(name: Username) -> Self {
        Self::Username(name)
    }
}

impl Person {
    /// The address or the username, in lower case.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Email(email) => email.as_str(),
            Self::Username(name) => name.as_str(),
        }
    }

    /// The kind of identifier this person is named by.
    #[must_use]
    pub fn kind(&self) -> IdentityKind {
        match self {
            Self::Email(_) => IdentityKind::Email,
            Self::Username(_) => IdentityKind::Username,
        }
    }

    /// The address, when the person is named by one.
    #[must_use]
    pub fn as_email(&self) -> Option<&Email> {
        match self {
            Self::Email(email) => Some(email),
            Self::Username(_) => None,
        }
    }

    /// The id that grants, team members and tokens are keyed by: the
    /// address or the username, in lower case.
    #[must_use]
    pub fn principal_id(&self) -> PrincipalId {
        match self {
            Self::Email(email) => email.principal_id(),
            Self::Username(name) => name.0.clone(),
        }
    }
}

impl fmt::Debug for Person {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Person(<hidden>)")
    }
}

#[cfg(test)]
mod tests;

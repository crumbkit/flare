//! The allow-list: who may sign in, as which principal, and what each one
//! may reach.

use crate::config::ConfigError;
use hangar_core::{PrincipalId, Resource};
use std::collections::HashSet;
use std::fmt;

/// An email address, in lower case.
///
/// Shortcut: the parser checks only that the address has one `@` between a
/// non-empty local part and a non-empty domain, at most 254 bytes and no
/// whitespace or control character. It does not check RFC 5321 syntax,
/// because the provider has verified the address and the crate only
/// compares it with the allow-list. Its `Debug` output hides the address.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Email(String);

impl TryFrom<&str> for Email {
    type Error = ConfigError;

    /// Admits one address and keeps it in lower case.
    fn try_from(text: &str) -> Result<Self, ConfigError> {
        let mut halves = text.split('@');
        let (Some(local), Some(domain), None) = (halves.next(), halves.next(), halves.next())
        else {
            return Err(ConfigError::Email);
        };
        let clean = !text
            .chars()
            .any(|character| character.is_whitespace() || character.is_control());
        if local.is_empty() || domain.is_empty() || text.len() > 254 || !clean {
            return Err(ConfigError::Email);
        }
        Ok(Self(text.to_lowercase()))
    }
}

#[cfg(test)]
impl Email {
    /// The address in lower case.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Email {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Email(<hidden>)")
    }
}

/// What a principal on the allow-list may reach, for read and for write.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Reach {
    /// Every resource.
    Everything,
    /// Only the resources listed.
    Only(Vec<Resource>),
}

impl Reach {
    /// Whether this reach includes `resource`.
    pub(crate) fn reaches(&self, resource: &Resource) -> bool {
        match self {
            Self::Everything => true,
            Self::Only(resources) => resources.contains(resource),
        }
    }
}

/// One person on the allow-list.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AllowEntry {
    email: Email,
    principal: PrincipalId,
    reach: Reach,
}

impl AllowEntry {
    /// The person with the verified email `email` signs in as `principal`
    /// and may reach `reach`.
    #[must_use]
    pub fn new(email: Email, principal: PrincipalId, reach: Reach) -> Self {
        Self {
            email,
            principal,
            reach,
        }
    }

    /// The principal that this person signs in as.
    pub(crate) fn principal(&self) -> &PrincipalId {
        &self.principal
    }

    /// What this person may reach.
    pub(crate) fn reach(&self) -> &Reach {
        &self.reach
    }
}

/// The people who may sign in. Each email and each principal appears once.
/// Its `Debug` output gives only the number of entries.
#[derive(Clone, Default)]
pub struct AllowList {
    entries: Vec<AllowEntry>,
}

impl AllowList {
    /// Builds the list.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::DuplicateEmail`] or
    /// [`ConfigError::DuplicatePrincipal`] when two entries share one.
    pub fn new(entries: Vec<AllowEntry>) -> Result<Self, ConfigError> {
        let mut emails = HashSet::new();
        let mut principals = HashSet::new();
        for entry in &entries {
            if !emails.insert(&entry.email) {
                return Err(ConfigError::DuplicateEmail);
            }
            if !principals.insert(&entry.principal) {
                return Err(ConfigError::DuplicatePrincipal);
            }
        }
        Ok(Self { entries })
    }

    /// The entry for `email`.
    pub(crate) fn by_email(&self, email: &Email) -> Option<&AllowEntry> {
        self.entries.iter().find(|entry| entry.email == *email)
    }

    /// The entry for `principal`.
    pub(crate) fn by_principal(&self, principal: &PrincipalId) -> Option<&AllowEntry> {
        self.entries
            .iter()
            .find(|entry| entry.principal == *principal)
    }
}

impl fmt::Debug for AllowList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AllowList")
            .field("entries", &self.entries.len())
            .finish()
    }
}

#[cfg(test)]
mod tests;

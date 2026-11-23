//! [`Email`], a verified address that sign-in and grants compare.

use crate::error::{ParseError, Subject};
use crate::name::PrincipalId;
use std::fmt;

/// The longest address, in bytes.
const MAX_EMAIL: usize = 254;

/// An email address of visible ASCII characters, held in lower case.
///
/// Shortcut: the parser checks only that the address has one `@` between a
/// non-empty local part and a non-empty domain, at most 254 bytes and only
/// visible ASCII characters. It does not check RFC 5321 syntax, because the
/// provider has verified the address and flare only compares it. Dots and
/// plus tags stay significant, so `a.lice@x.com` and `alice+t@x.com` are
/// two addresses that differ from `alice@x.com`.
///
/// Its `Debug` output hides the address.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Email(PrincipalId);

impl TryFrom<&str> for Email {
    type Error = ParseError;

    /// Admits one address and keeps it in lower case.
    fn try_from(text: &str) -> Result<Self, ParseError> {
        let subject = Subject::Email;
        if text.is_empty() {
            return Err(ParseError::Empty { subject });
        }
        if text.len() > MAX_EMAIL {
            return Err(ParseError::TooLong {
                subject,
                max: MAX_EMAIL,
            });
        }
        let malformed = ParseError::Malformed {
            subject,
            rule: "one address of visible ASCII characters with one @ between two parts",
        };
        let mut halves = text.split('@');
        let (Some(local), Some(domain), None) = (halves.next(), halves.next(), halves.next())
        else {
            return Err(malformed);
        };
        // Only ASCII is admitted: Unicode lower-casing maps some other
        // characters onto ASCII letters, such as U+212A KELVIN SIGN onto k,
        // which would let one address match another's grant.
        let clean = text.bytes().all(|byte| byte.is_ascii_graphic());
        if local.is_empty() || domain.is_empty() || !clean {
            return Err(malformed);
        }
        // An address of at most 254 visible ASCII bytes is always a valid
        // principal id, whose rule is 1 to 256 of them.
        PrincipalId::try_from(text.to_ascii_lowercase().as_str())
            .map(Self)
            .map_err(|_| malformed)
    }
}

impl Email {
    /// The address in lower case.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// The principal id that sign-in gives the person with this address:
    /// the address in lower case. Grants are keyed by it.
    #[must_use]
    pub fn principal_id(&self) -> PrincipalId {
        self.0.clone()
    }
}

impl fmt::Debug for Email {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Email(<hidden>)")
    }
}

#[cfg(test)]
mod tests;

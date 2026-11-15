//! Who may sign in, as which principal, and what each one may reach: the
//! allow-list of emails, or every account of one Google Workspace domain.

use crate::config::ConfigError;
use crate::refusal::SignInRefusal;
use hangar_core::{PrincipalId, Resource};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;

/// An email address of ASCII characters, in lower case.
///
/// Shortcut: the parser checks only that the address has one `@` between a
/// non-empty local part and a non-empty domain, at most 254 bytes and only
/// visible ASCII characters. It does not check RFC 5321 syntax,
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
        // Only ASCII is admitted: Unicode lower-casing maps some other
        // characters onto ASCII letters, such as U+212A KELVIN SIGN onto k,
        // which would let one address match another's allow-list entry.
        let clean = text.bytes().all(|byte| byte.is_ascii_graphic());
        if local.is_empty() || domain.is_empty() || text.len() > 254 || !clean {
            return Err(ConfigError::Email);
        }
        Ok(Self(text.to_ascii_lowercase()))
    }
}

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

/// The longest DNS name, in bytes, without a trailing dot.
const MAX_DOMAIN: usize = 253;
/// The longest DNS label, in bytes.
const MAX_LABEL: usize = 63;

/// A Google Workspace domain, such as `example.com`: a DNS name of at least
/// two labels, held in lower case. Its `Debug` output hides the name.
///
/// Shortcut: only ASCII names are admitted, and no internationalised name
/// is converted to its `xn--` form, because Google sends `hd` in that ASCII
/// form and the crate only compares the two. A deployer configures an
/// internationalised domain by its `xn--` form.
#[derive(Clone, PartialEq, Eq)]
pub struct WorkspaceDomain(String);

/// Whether `label` is 1 to 63 letters, digits or hyphens that neither
/// starts nor ends with a hyphen.
fn is_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    let clean = bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-');
    (1..=MAX_LABEL).contains(&bytes.len())
        && clean
        && bytes.first() != Some(&b'-')
        && bytes.last() != Some(&b'-')
}

impl TryFrom<&str> for WorkspaceDomain {
    type Error = ConfigError;

    /// Admits 1 to 253 ASCII bytes of at least two labels joined by single
    /// dots, and keeps them in lower case.
    fn try_from(text: &str) -> Result<Self, ConfigError> {
        let labels = text.split('.').count();
        if text.len() > MAX_DOMAIN || labels < 2 || !text.split('.').all(is_label) {
            return Err(ConfigError::Domain);
        }
        Ok(Self(text.to_ascii_lowercase()))
    }
}

impl WorkspaceDomain {
    /// The domain in lower case.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether a signed `hd` claim names this domain: a JSON string equal
    /// to it without regard to ASCII letter case. Nothing is trimmed, and no
    /// subdomain, parent domain or suffix matches.
    fn is_named_by(&self, hd: &Value) -> bool {
        matches!(hd, Value::String(hd) if hd.eq_ignore_ascii_case(&self.0))
    }
}

impl fmt::Debug for WorkspaceDomain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WorkspaceDomain(<hidden>)")
    }
}

/// Every verified account of one Google Workspace domain may sign in, and
/// each one may reach the same resources.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct DomainRule {
    domain: WorkspaceDomain,
    reach: Reach,
}

impl DomainRule {
    /// The accounts whose signed `hd` claim names `domain` sign in as their
    /// email in lower case, and each may reach `reach`.
    #[must_use]
    pub fn new(domain: WorkspaceDomain, reach: Reach) -> Self {
        Self { domain, reach }
    }

    /// What every admitted principal may reach.
    pub(crate) fn reach(&self) -> &Reach {
        &self.reach
    }

    /// The principal of a verified `email` whose ID token carries the `hd`
    /// claim `hd`.
    ///
    /// Shortcut: `hd` is a Google claim, and the rule does not check that
    /// the issuer is Google. With another issuer, the rule trusts that
    /// issuer's `hd`.
    fn principal(&self, email: &str, hd: Option<&Value>) -> Result<PrincipalId, SignInRefusal> {
        let hd = hd.ok_or(SignInRefusal::NoHostedDomain)?;
        if !self.domain.is_named_by(hd) {
            return Err(SignInRefusal::OtherHostedDomain);
        }
        // The email's own domain is not compared: an account of the
        // organisation may have an email in a secondary domain.
        let email = Email::try_from(email).map_err(|_| SignInRefusal::NotAllowed)?;
        PrincipalId::try_from(email.as_str()).map_err(|_| SignInRefusal::NotAllowed)
    }
}

/// Who may sign in.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Admit {
    /// The emails on the list. Each entry gives its principal and reach,
    /// and the `hd` claim is ignored.
    Emails(AllowList),
    /// Every verified account of one Google Workspace domain, as the signed
    /// `hd` claim of its ID token says. It is meant for the Google issuer.
    Domain(DomainRule),
}

impl Admit {
    /// The principal that a verified `email`, with the `hd` claim `hd`,
    /// signs in as.
    pub(crate) fn principal(
        &self,
        email: &str,
        hd: Option<&Value>,
    ) -> Result<PrincipalId, SignInRefusal> {
        match self {
            Self::Emails(allow) => {
                let email = Email::try_from(email).map_err(|_| SignInRefusal::NotAllowed)?;
                allow
                    .by_email(&email)
                    .map(|entry| entry.principal().clone())
                    .ok_or(SignInRefusal::NotAllowed)
            }
            Self::Domain(rule) => rule.principal(email, hd),
        }
    }

    /// The domain to send as the `hd` parameter of the authorisation
    /// request. Google then offers only that organisation's accounts. A
    /// person can change the parameter, so it admits nobody.
    pub(crate) fn hosted_domain_hint(&self) -> Option<&str> {
        match self {
            Self::Emails(_) => None,
            Self::Domain(rule) => Some(rule.domain.as_str()),
        }
    }
}

#[cfg(test)]
mod tests;

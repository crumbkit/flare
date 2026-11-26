//! Who may sign in, as which principal, and what each one may reach: the
//! allow-list of emails, or every account of one Google Workspace domain.

use crate::config::ConfigError;
use crate::refusal::SignInRefusal;
use flare_core::{Email, Principal, PrincipalId, Resource};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;

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

    /// Whether `email` is an address of this domain: the part after its
    /// `@` equals the domain. No subdomain, parent domain or suffix
    /// matches.
    fn owns(&self, email: &Email) -> bool {
        email
            .as_str()
            .rsplit_once('@')
            .is_some_and(|(_, domain)| domain.eq_ignore_ascii_case(&self.0))
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
    /// claim `hd`: its id is the email in lower case, and it carries the
    /// email.
    ///
    /// Shortcut: `hd` is a Google claim, and the rule does not check that
    /// the issuer is Google. With another issuer, the rule trusts that
    /// issuer's `hd`.
    fn principal(&self, email: &str, hd: Option<&Value>) -> Result<Principal, SignInRefusal> {
        let hd = hd.ok_or(SignInRefusal::NoHostedDomain)?;
        if !self.domain.is_named_by(hd) {
            return Err(SignInRefusal::OtherHostedDomain);
        }
        // The email's own domain is not compared: an account of the
        // organisation may have an email in a secondary domain.
        let email = Email::try_from(email).map_err(|_| SignInRefusal::NotAllowed)?;
        Ok(Principal::with_email(email.principal_id(), email))
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
    /// Every account whose email the provider verified, each signing in
    /// as its own address and reaching the resources given. It lets people
    /// outside the operator's organisation sign in and read the shares
    /// granted to them. This rule does not widen who may create a share,
    /// which is the server's creators setting, except that a creators
    /// setting of `"anyone admitted"` under this rule lets every such
    /// account create shares: with the Google issuer, any Google account.
    Anyone(Reach),
}

impl Admit {
    /// The principal that a verified `email`, with the `hd` claim `hd`,
    /// signs in as. It carries the verified email, in lower case, which
    /// shares are decided by, whatever principal id the rule gives.
    pub(crate) fn principal(
        &self,
        email: &str,
        hd: Option<&Value>,
    ) -> Result<Principal, SignInRefusal> {
        match self {
            Self::Emails(allow) => {
                let email = Email::try_from(email).map_err(|_| SignInRefusal::NotAllowed)?;
                let id = allow
                    .by_email(&email)
                    .map(|entry| entry.principal().clone())
                    .ok_or(SignInRefusal::NotAllowed)?;
                Ok(Principal::with_email(id, email))
            }
            Self::Domain(rule) => rule.principal(email, hd),
            Self::Anyone(_) => {
                let email = Email::try_from(email).map_err(|_| SignInRefusal::NotAllowed)?;
                Ok(Principal::with_email(email.principal_id(), email))
            }
        }
    }

    /// Whether the rule admits `email`, judged from the address alone, as
    /// a CLI token's check does on every request. Under
    /// [`Admit::Emails`] the address must be on the list. The
    /// [`Admit::Domain`] and [`Admit::Anyone`] rules admit every address,
    /// because the `hd` claim that decides a domain sign-in is only in an
    /// ID token.
    #[must_use]
    pub fn admits(&self, email: &Email) -> bool {
        match self {
            Self::Emails(allow) => allow.by_email(email).is_some(),
            Self::Domain(_) | Self::Anyone(_) => true,
        }
    }

    /// The principal of an address that a proxy's signed token names,
    /// judged from the address alone, because such a token has no `hd`
    /// claim. Under [`Admit::Emails`] it is the list entry's principal,
    /// under [`Admit::Domain`] the address of an address whose domain is the
    /// rule's, and under [`Admit::Anyone`] the address. `None` when the rule
    /// does not admit the address.
    pub(crate) fn principal_of_address(&self, email: Email) -> Option<Principal> {
        let id = match self {
            Self::Emails(allow) => allow.by_email(&email)?.principal().clone(),
            Self::Domain(rule) if rule.domain.owns(&email) => email.principal_id(),
            Self::Domain(_) => return None,
            Self::Anyone(_) => email.principal_id(),
        };
        Some(Principal::with_email(id, email))
    }

    /// Whether a principal that [`Admit::principal_of_address`] gave
    /// reaches `resource`: the list entry's reach, or the rule's reach for
    /// an address the rule still admits.
    pub(crate) fn address_reaches(&self, principal: &Principal, resource: &Resource) -> bool {
        match self {
            Self::Emails(allow) => allow
                .by_principal(principal.id())
                .is_some_and(|entry| entry.reach().reaches(resource)),
            Self::Domain(rule) => {
                principal
                    .email()
                    .is_some_and(|email| rule.domain.owns(email))
                    && rule.reach.reaches(resource)
            }
            Self::Anyone(reach) => reach.reaches(resource),
        }
    }

    /// The Google Workspace domain that a domain rule admits, or `None`
    /// for any other rule.
    #[must_use]
    pub fn domain(&self) -> Option<&WorkspaceDomain> {
        match self {
            Self::Emails(_) | Self::Anyone(_) => None,
            Self::Domain(rule) => Some(&rule.domain),
        }
    }

    /// The domain to send as the `hd` parameter of the authorisation
    /// request. Google then offers only that organisation's accounts. A
    /// person can change the parameter, so it admits nobody.
    pub(crate) fn hosted_domain_hint(&self) -> Option<&str> {
        match self {
            Self::Emails(_) | Self::Anyone(_) => None,
            Self::Domain(rule) => Some(rule.domain.as_str()),
        }
    }
}

#[cfg(test)]
mod tests;

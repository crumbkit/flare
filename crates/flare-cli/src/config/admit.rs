//! The `admit` rule of the `oidc` and `proxy` sections: a list of emails,
//! one domain, or anyone, each with what it may reach.

use super::reach::{ReachSection, reach};
use flare_auth_oidc::{Admit, AllowEntry, AllowList, DomainRule, Email, WorkspaceDomain};
use flare_core::PrincipalId;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum AdmitSection {
    Emails(EmailsSection),
    Domain(DomainSection),
    Anyone(AnyoneSection),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AnyoneSection {
    anyone: bool,
    reach: ReachSection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmailsSection {
    emails: Vec<EmailSection>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailSection {
    email: String,
    principal: String,
    reach: ReachSection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DomainSection {
    domain: String,
    reach: ReachSection,
}

impl AdmitSection {
    /// Whether the rule admits anyone.
    pub(super) fn is_anyone(&self) -> bool {
        matches!(self, Self::Anyone(_))
    }
}

/// The rule that `section`, the `admit` key of the section `key` names,
/// gives.
///
/// # Errors
///
/// Returns the problem.
pub(super) fn admit(section: AdmitSection, key: &str) -> Result<Admit, String> {
    Ok(match section {
        AdmitSection::Emails(list) => {
            let mut entries = Vec::new();
            for entry in list.emails {
                let email =
                    Email::try_from(entry.email.as_str()).map_err(|error| error.to_string())?;
                let principal = PrincipalId::try_from(entry.principal.as_str())
                    .map_err(|error| format!("a principal is not valid: {error}"))?;
                entries.push(AllowEntry::new(email, principal, reach(entry.reach)?));
            }
            Admit::Emails(AllowList::new(entries).map_err(|error| error.to_string())?)
        }
        AdmitSection::Domain(rule) => {
            let domain = WorkspaceDomain::try_from(rule.domain.as_str())
                .map_err(|error| error.to_string())?;
            Admit::Domain(DomainRule::new(domain, reach(rule.reach)?))
        }
        AdmitSection::Anyone(rule) if rule.anyone => Admit::Anyone(reach(rule.reach)?),
        AdmitSection::Anyone(_) => {
            return Err(format!(
                "{key}.admit.anyone must be true; to admit fewer people, list emails or a domain"
            ));
        }
    })
}

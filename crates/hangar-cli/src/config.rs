//! The config file: `hangar serve --config <path>`, else
//! `$XDG_CONFIG_HOME/hangar/config.json`, else `~/.config/hangar/config.json`.
//!
//! A missing file means loopback only. Every object refuses unknown keys,
//! and a `client_secret` key anywhere is refused with a message that names
//! the Keychain service, because the secret lives only in the Keychain.
//! No message repeats a value from the file.

use crate::codes::Failed;
use crate::env::Snapshot;
use hangar_auth_oidc::{
    Admit, AllowEntry, AllowList, ClientId, DomainRule, Email, IssuerUrl, OidcConfig, Reach,
    RedirectUri, WorkspaceDomain,
};
use hangar_core::{BindingName, PageName, PrincipalId, RecordPath, Resource};
use serde::Deserialize;
use serde_json::Value;
use std::io::Read as _;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests;

/// The only redirect URI: the sign-in host's callback.
const REDIRECT: &str = "http://localhost:8740/auth/oidc/callback";

/// The Keychain service that holds the client secret.
pub(crate) const KEYCHAIN_SERVICE: &str = "hangar-oidc-google";

/// A parsed config.
#[derive(Debug)]
pub(crate) struct Config {
    /// The OpenID Connect settings, when the file has them.
    pub(crate) oidc: Option<OidcConfig>,
}

/// Why a config's text was refused.
#[derive(Debug)]
pub(crate) enum Problem {
    /// The text is not a hangar config.
    Invalid(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    oidc: Option<OidcSection>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OidcSection {
    issuer: String,
    client_id: String,
    admit: AdmitSection,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AdmitSection {
    Emails(EmailsSection),
    Domain(DomainSection),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmailsSection {
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
struct DomainSection {
    domain: String,
    reach: ReachSection,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ReachSection {
    Word(String),
    Only(Vec<ResourceSection>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResourceSection {
    page: Option<String>,
    binding: Option<String>,
    record: Option<String>,
}

/// The config file's path: `given`, else under `XDG_CONFIG_HOME`, else
/// under `HOME`. `None` when neither variable is an absolute path.
pub(crate) fn path(given: Option<&Path>, env: &Snapshot) -> Option<PathBuf> {
    if let Some(given) = given {
        return Some(given.to_path_buf());
    }
    let absolute = |name| {
        env.get(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    absolute("XDG_CONFIG_HOME")
        .or_else(|| absolute("HOME").map(|home| home.join(".config")))
        .map(|base| base.join("hangar").join("config.json"))
}

/// The largest config file hangar reads, in bytes.
const MAX_BYTES: u64 = 64 * 1024;

/// Reads and parses the config file. A missing file at the default path
/// means loopback only; a missing file that `--config` names is refused.
///
/// # Errors
///
/// Returns [`Failed::ConfigUnreadable`] or [`Failed::ConfigInvalid`].
pub(crate) fn load(given: Option<&Path>, env: &Snapshot) -> Result<Config, Failed> {
    let Some(path) = self::path(given, env) else {
        return Ok(Config { oidc: None });
    };
    let shown = path.display().to_string();
    let unreadable = |problem: String| Failed::ConfigUnreadable {
        path: shown.clone(),
        problem,
    };
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && given.is_none() => {
            return Ok(Config { oidc: None });
        }
        Err(error) => return Err(unreadable(error.to_string())),
    };
    let mut text = String::new();
    file.take(MAX_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| unreadable(error.to_string()))?;
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        return Err(unreadable(format!("it is longer than {MAX_BYTES} bytes")));
    }
    parse(&text).map_err(|Problem::Invalid(problem)| Failed::ConfigInvalid {
        path: shown.clone(),
        problem,
    })
}

/// Parses a config's text.
///
/// # Errors
///
/// Returns [`Problem::Invalid`] with what is wrong.
pub(crate) fn parse(text: &str) -> Result<Config, Problem> {
    let invalid = |text: &str| Problem::Invalid(text.to_owned());
    let value: Value = serde_json::from_str(text).map_err(|error| {
        invalid(&format!(
            "it is not JSON (line {}, column {})",
            error.line(),
            error.column()
        ))
    })?;
    if holds_client_secret(&value) {
        return Err(invalid(&format!(
            "it holds a client_secret key; remove it and keep the secret in the Keychain service {KEYCHAIN_SERVICE}"
        )));
    }
    let file: File = serde_json::from_value(value).map_err(|error| invalid(&error.to_string()))?;
    let oidc = file.oidc.map(oidc).transpose().map_err(Problem::Invalid)?;
    Ok(Config { oidc })
}

/// Whether any object in `value` has a `client_secret` key.
fn holds_client_secret(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.contains_key("client_secret") || map.values().any(holds_client_secret)
        }
        Value::Array(items) => items.iter().any(holds_client_secret),
        _ => false,
    }
}

fn oidc(section: OidcSection) -> Result<OidcConfig, String> {
    let issuer = IssuerUrl::try_from(section.issuer.as_str()).map_err(|error| error.to_string())?;
    let client =
        ClientId::try_from(section.client_id.as_str()).map_err(|error| error.to_string())?;
    let redirect = RedirectUri::try_from(REDIRECT).map_err(|error| error.to_string())?;
    let admit = match section.admit {
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
    };
    Ok(OidcConfig::new(issuer, client, redirect, admit))
}

fn reach(section: ReachSection) -> Result<Reach, String> {
    match section {
        ReachSection::Word(word) if word == "everything" => Ok(Reach::Everything),
        ReachSection::Word(_) => Err("reach must be \"everything\" or a list".to_owned()),
        ReachSection::Only(items) => items
            .into_iter()
            .map(resource)
            .collect::<Result<Vec<_>, _>>()
            .map(Reach::Only),
    }
}

fn resource(section: ResourceSection) -> Result<Resource, String> {
    let bad = |error: hangar_core::ParseError| format!("a reach entry is not valid: {error}");
    match (section.page, section.binding, section.record) {
        (Some(page), None, None) => PageName::try_from(page.as_str())
            .map(Resource::Page)
            .map_err(bad),
        (None, Some(binding), None) => BindingName::try_from(binding.as_str())
            .map(Resource::Binding)
            .map_err(bad),
        (None, None, Some(record)) => RecordPath::try_from(record.as_str())
            .map(Resource::Record)
            .map_err(bad),
        _ => Err("each reach entry names exactly one page, binding or record".to_owned()),
    }
}

//! The config file: `flare serve --config <path>`, else
//! `$XDG_CONFIG_HOME/flare/config.json`, else `~/.config/flare/config.json`.
//!
//! A missing file means the local site with loopback sign-in only. A
//! `hosted` section makes `flare serve` serve the hosted site, behind a
//! TLS proxy, and needs an `oidc` section with `client_secret_file`,
//! because a host has no Keychain. A `stores` key in the hosted section
//! adds the share API, which `creators` limits, `names` serves and
//! `get_flare` links the reading page's Get Flare link to. A
//! `log` section with `"metrics": true` turns on the metric lines of
//! docs/metrics.md; they are off without it. Every object refuses
//! unknown keys, and a
//! `client_secret` key anywhere is refused, because the secret lives only
//! in the Keychain or in the file `client_secret_file` names. A refusal
//! names the key, and no message repeats a value from the file.

use crate::codes::Failed;
use crate::env::Snapshot;
use flare_auth_oidc::{
    Admit, AllowEntry, AllowList, ClientId, DomainRule, Email, IssuerUrl, OidcConfig, RedirectUri,
    WorkspaceDomain,
};
use flare_core::PrincipalId;
use flare_server::PublicOrigin;
use serde::Deserialize;
use serde_json::Value;
use std::io::Read as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

mod reach;
mod sharing;
#[cfg(test)]
mod tests;

use reach::{ReachSection, reach};
pub(crate) use sharing::Sharing;

/// The local site's only redirect URI: the sign-in host's callback.
const REDIRECT: &str = "http://localhost:8740/auth/oidc/callback";

/// The Keychain service that holds the client secret.
pub(crate) const KEYCHAIN_SERVICE: &str = "flare-oidc-google";

/// A parsed config.
#[derive(Debug)]
pub(crate) struct Config {
    /// The site `flare serve` serves.
    pub(crate) site: Site,
    /// Whether the server writes a metric line for each counted event.
    pub(crate) metrics: bool,
}

/// Which site the config asks for.
#[derive(Debug)]
pub(crate) enum Site {
    /// The local site, with OpenID Connect when the file has an `oidc`
    /// section.
    Local {
        /// The OpenID Connect settings.
        oidc: Option<LocalOidc>,
    },
    /// The hosted site.
    Hosted(Hosted),
}

/// The local site's OpenID Connect settings and where its secret is.
#[derive(Debug)]
pub(crate) struct LocalOidc {
    /// The settings.
    pub(crate) config: OidcConfig,
    /// Where the client secret is.
    pub(crate) secret: Secret,
}

/// Where the client secret is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Secret {
    /// The Keychain service [`KEYCHAIN_SERVICE`].
    Keychain,
    /// This file.
    File(PathBuf),
}

/// The hosted site's settings.
#[derive(Debug)]
pub(crate) struct Hosted {
    /// The public origin, such as `https://flare.example.com`.
    pub(crate) origin: PublicOrigin,
    /// The address the plain-HTTP listener binds, which only the proxy
    /// reaches.
    pub(crate) listen: SocketAddr,
    /// The OpenID Connect settings, whose redirect URI is
    /// `<origin>/auth/oidc/callback`.
    pub(crate) oidc: OidcConfig,
    /// The file that holds the client secret.
    pub(crate) secret_file: PathBuf,
    /// The share API's settings, when the section names its stores.
    pub(crate) sharing: Option<Box<Sharing>>,
}

/// Why a config's text was refused.
#[derive(Debug)]
pub(crate) enum Problem {
    /// The text is not a flare config.
    Invalid(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    hosted: Option<HostedSection>,
    oidc: Option<OidcSection>,
    log: Option<LogSection>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogSection {
    #[serde(default)]
    metrics: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostedSection {
    origin: String,
    listen: String,
    stores: Option<flare_aws::HostedConfig>,
    creators: Option<Value>,
    names: Option<std::collections::BTreeMap<String, String>>,
    get_flare: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OidcSection {
    issuer: String,
    client_id: String,
    client_secret_file: Option<String>,
    admit: AdmitSection,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AdmitSection {
    Emails(EmailsSection),
    Domain(DomainSection),
    Anyone(AnyoneSection),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnyoneSection {
    anyone: bool,
    reach: ReachSection,
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
        .map(|base| base.join("flare").join("config.json"))
}

/// The largest config file flare reads, in bytes.
const MAX_BYTES: u64 = 64 * 1024;

/// Reads and parses the config file. A missing file at the default path
/// means loopback only; a missing file that `--config` names is refused.
///
/// # Errors
///
/// Returns [`Failed::ConfigUnreadable`] or [`Failed::ConfigInvalid`].
pub(crate) fn load(given: Option<&Path>, env: &Snapshot) -> Result<Config, Failed> {
    let local = || Config {
        site: Site::Local { oidc: None },
        metrics: false,
    };
    let Some(path) = self::path(given, env) else {
        return Ok(local());
    };
    let shown = path.display().to_string();
    let unreadable = |problem: String| Failed::ConfigUnreadable {
        path: shown.clone(),
        problem,
    };
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && given.is_none() => {
            return Ok(local());
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
            "it holds a client_secret key; remove it and keep the secret in the Keychain service {KEYCHAIN_SERVICE}, or in a file that oidc.client_secret_file names"
        )));
    }
    let file: File = serde_json::from_value(value).map_err(|error| invalid(&error.to_string()))?;
    let metrics = file.log.is_some_and(|log| log.metrics);
    let site = match (file.hosted, file.oidc) {
        (None, oidc) => Site::Local {
            oidc: oidc.map(local_oidc).transpose().map_err(Problem::Invalid)?,
        },
        (Some(hosted), Some(oidc)) => {
            Site::Hosted(self::hosted(hosted, oidc).map_err(Problem::Invalid)?)
        }
        (Some(_), None) => {
            return Err(invalid(
                "a hosted section needs an oidc section, because the hosted site signs people in only with OpenID Connect",
            ));
        }
    };
    Ok(Config { site, metrics })
}

/// The secret file's path: absolute, or a refusal that names the key.
fn secret_file(text: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(text);
    if path.is_absolute() {
        Ok(path)
    } else {
        Err("oidc.client_secret_file must be an absolute path".to_owned())
    }
}

fn local_oidc(section: OidcSection) -> Result<LocalOidc, String> {
    let secret = match &section.client_secret_file {
        Some(text) => Secret::File(secret_file(text)?),
        None => Secret::Keychain,
    };
    let redirect = RedirectUri::try_from(REDIRECT).map_err(|error| error.to_string())?;
    Ok(LocalOidc {
        config: oidc(section, redirect)?,
        secret,
    })
}

fn hosted(section: HostedSection, oidc_section: OidcSection) -> Result<Hosted, String> {
    let origin = PublicOrigin::try_from(section.origin.as_str())
        .map_err(|error| format!("hosted.origin is not valid: {error}"))?;
    let listen = section.listen.parse::<SocketAddr>().map_err(|_| {
        "hosted.listen must be an IP address and a port, such as 0.0.0.0:8080 or [::]:8080"
            .to_owned()
    })?;
    let secret_file = match &oidc_section.client_secret_file {
        Some(text) => secret_file(text)?,
        None => {
            return Err(
                "a hosted section needs oidc.client_secret_file, the absolute path of the file that holds the client secret, because a host has no Keychain"
                    .to_owned(),
            );
        }
    };
    let anyone = matches!(oidc_section.admit, AdmitSection::Anyone(_));
    let sharing = sharing::sharing(
        sharing::Keys {
            stores: section.stores,
            creators: section.creators,
            names: section.names,
            get_flare: section.get_flare,
        },
        anyone,
    )?
    .map(Box::new);
    let redirect =
        RedirectUri::try_from(origin.redirect_uri().as_str()).map_err(|error| error.to_string())?;
    Ok(Hosted {
        oidc: oidc(oidc_section, redirect)?,
        origin,
        listen,
        secret_file,
        sharing,
    })
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

fn oidc(section: OidcSection, redirect: RedirectUri) -> Result<OidcConfig, String> {
    let issuer = IssuerUrl::try_from(section.issuer.as_str()).map_err(|error| error.to_string())?;
    let client =
        ClientId::try_from(section.client_id.as_str()).map_err(|error| error.to_string())?;
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
        AdmitSection::Anyone(rule) if rule.anyone => Admit::Anyone(reach(rule.reach)?),
        AdmitSection::Anyone(_) => {
            return Err(
                "oidc.admit.anyone must be true; to admit fewer people, list emails or a domain"
                    .to_owned(),
            );
        }
    };
    Ok(OidcConfig::new(issuer, client, redirect, admit))
}

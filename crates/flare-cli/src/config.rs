//! The config file: `flare serve --config <path>`, else
//! `$XDG_CONFIG_HOME/flare/config.json`, else `~/.config/flare/config.json`.
//!
//! A missing file means the local site with loopback sign-in only. A
//! `hosted` section makes `flare serve` serve the hosted site, behind a
//! TLS proxy, and needs exactly one of an `oidc` section with
//! `client_secret_file`, because a host has no Keychain, and a `proxy`
//! section, which trusts the signed token of a single sign-on proxy in
//! front of the site. A `client` section names the server that the share
//! commands call and the token command that gives them its token. A `stores` key in the hosted section
//! adds the share API, which `creators` limits, `names` serves and
//! `get_flare` links the reading page's Get Flare link to.
//! `identity` says whether people are named by email or by username, and
//! `everyone_signed_in` lets an everyone grant under an `anyone` rule open
//! to every signed-in person, and `teams` says where the teams come from. A
//! `log` section with `"metrics": true` turns on the metric lines of
//! docs/metrics.md; they are off without it. Every object refuses
//! unknown keys, and a
//! `client_secret` key anywhere is refused, because the secret lives only
//! in the Keychain or in the file `client_secret_file` names. A refusal
//! names the key, and no message repeats a value from the file.

use crate::codes::Failed;
use crate::env::Snapshot;
use flare_auth_oidc::{ClientId, Identity, IssuerUrl, OidcConfig, RedirectUri};
use flare_core::IdentityKind;
use flare_server::PublicOrigin;
use serde::Deserialize;
use serde_json::Value;
use std::io::Read as _;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

mod admit;
mod client;
mod identity;
mod proxy;
mod reach;
mod sharing;
mod sign_in;
mod teams;
#[cfg(test)]
mod tests;

use admit::{AdmitSection, admit};
pub(crate) use client::{Client, TokenCommand};
use proxy::ProxySection;
pub(crate) use sharing::{HostedCreators, Sharing};
pub(crate) use sign_in::SignIn;
pub(crate) use teams::TeamsSource;

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
    /// The share commands' server and token command, when the file has a
    /// `client` section.
    pub(crate) client: Option<Client>,
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
    /// How the site signs people in.
    pub(crate) sign_in: SignIn,
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
    proxy: Option<ProxySection>,
    client: Option<client::ClientSection>,
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
    identity: Option<Value>,
    everyone_signed_in: Option<bool>,
    teams: Option<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OidcSection {
    issuer: String,
    client_id: String,
    client_secret_file: Option<String>,
    admit: AdmitSection,
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
        client: None,
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
    let client = file
        .client
        .map(client::client)
        .transpose()
        .map_err(Problem::Invalid)?;
    let site = match (file.hosted, file.oidc, file.proxy) {
        (None, oidc, None) => Site::Local {
            oidc: oidc.map(local_oidc).transpose().map_err(Problem::Invalid)?,
        },
        (None, _, Some(_)) => {
            return Err(invalid(
                "a proxy section needs a hosted section, because only the hosted site signs people in through a proxy",
            ));
        }
        (Some(hosted), Some(oidc), None) => {
            Site::Hosted(self::hosted(hosted, HostedSignIn::Oidc(oidc)).map_err(Problem::Invalid)?)
        }
        (Some(hosted), None, Some(proxy)) => Site::Hosted(
            self::hosted(hosted, HostedSignIn::Proxy(proxy)).map_err(Problem::Invalid)?,
        ),
        (Some(_), oidc, _) => {
            let what = if oidc.is_some() { "both" } else { "neither" };
            return Err(invalid(&format!(
                "a hosted section takes exactly one of oidc and proxy, and this file has {what}"
            )));
        }
    };
    Ok(Config {
        site,
        metrics,
        client,
    })
}

/// The sign-in section that a hosted section came with.
enum HostedSignIn {
    Oidc(OidcSection),
    Proxy(ProxySection),
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
        config: oidc(section, redirect, Identity::Email)?,
        secret,
    })
}

fn hosted(section: HostedSection, sign_in: HostedSignIn) -> Result<Hosted, String> {
    let origin = PublicOrigin::try_from(section.origin.as_str())
        .map_err(|error| format!("hosted.origin is not valid: {error}"))?;
    let listen = section.listen.parse::<SocketAddr>().map_err(|_| {
        "hosted.listen must be an IP address and a port, such as 0.0.0.0:8080 or [::]:8080"
            .to_owned()
    })?;
    let identity = identity::identity(section.identity.as_ref())?;
    let anyone = match &sign_in {
        HostedSignIn::Oidc(oidc) => oidc.admit.is_anyone(),
        HostedSignIn::Proxy(proxy) => proxy.admits_anyone(),
    };
    let sharing = sharing::sharing(
        sharing::Keys {
            stores: section.stores,
            creators: section.creators,
            names: section.names,
            get_flare: section.get_flare,
            everyone_signed_in: section.everyone_signed_in,
            teams: section.teams,
        },
        anyone,
        identity.kind(),
    )?
    .map(Box::new);
    let sign_in = match sign_in {
        HostedSignIn::Oidc(oidc_section) => hosted_oidc(&origin, oidc_section, identity)?,
        HostedSignIn::Proxy(proxy) => SignIn::Proxy(proxy::proxy(proxy, identity)?),
    };
    Ok(Hosted {
        origin,
        listen,
        sign_in,
        sharing,
    })
}

/// The hosted site's OpenID Connect sign-in, which reads its client secret
/// from a file and names people as `identity` says.
fn hosted_oidc(
    origin: &PublicOrigin,
    section: OidcSection,
    identity: Identity,
) -> Result<SignIn, String> {
    let secret_file = match &section.client_secret_file {
        Some(text) => secret_file(text)?,
        None => {
            return Err(
                "a hosted section needs oidc.client_secret_file, the absolute path of the file that holds the client secret, because a host has no Keychain"
                    .to_owned(),
            );
        }
    };
    let redirect =
        RedirectUri::try_from(origin.redirect_uri().as_str()).map_err(|error| error.to_string())?;
    Ok(SignIn::Oidc {
        config: oidc(section, redirect, identity)?,
        secret_file,
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

/// The OpenID Connect settings. Its admit rule judges the verified email
/// under either kind, so its list entries are addresses.
fn oidc(
    section: OidcSection,
    redirect: RedirectUri,
    identity: Identity,
) -> Result<OidcConfig, String> {
    let issuer = IssuerUrl::try_from(section.issuer.as_str()).map_err(|error| error.to_string())?;
    let client =
        ClientId::try_from(section.client_id.as_str()).map_err(|error| error.to_string())?;
    let admit = admit(section.admit, "oidc", IdentityKind::Email)?;
    Ok(OidcConfig::new(issuer, client, redirect, admit).with_identity(identity))
}

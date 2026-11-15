//! What the authenticator is configured with: the issuer, the client, the
//! redirect URI and the allow-list.

use crate::allow::Admit;
use std::net::IpAddr;
use std::time::Duration;
use url::{Host, Url};

/// Why a configuration value was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// The issuer is not an https URL with a host and no user, query or
    /// fragment.
    #[error("the issuer must be an https URL with a host and no user, query or fragment")]
    Issuer,
    /// The client ID is empty, longer than 256 bytes or holds a character
    /// outside visible ASCII.
    #[error("the client ID must be 1 to 256 visible ASCII characters")]
    ClientId,
    /// The redirect URI is not an https URL or an http URL on a loopback
    /// host, or it has a user or a fragment.
    #[error("the redirect URI must be https, or http on a loopback host, with no user or fragment")]
    RedirectUri,
    /// The email is not one address of at most 254 bytes without spaces.
    #[error("an email must be one address of at most 254 bytes with no spaces")]
    Email,
    /// The allow-list names one email twice.
    #[error("the allow-list names one email twice")]
    DuplicateEmail,
    /// The allow-list names one principal twice.
    #[error("the allow-list names one principal twice")]
    DuplicatePrincipal,
    /// The domain is not a DNS name.
    #[error("the domain is not a DNS name")]
    Domain,
    /// The return-to value is longer than 2,048 bytes.
    #[error("the return-to value is longer than 2,048 bytes")]
    ReturnTo,
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => domain == "localhost",
        Some(Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
        Some(Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
        None => false,
    }
}

fn plain(url: &Url) -> bool {
    url.has_host()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}

/// The provider's issuer, such as `https://accounts.google.com`. It is kept
/// exactly as written, because the ID token's `iss` must equal it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuerUrl {
    text: String,
    url: Url,
    loopback_http: bool,
}

impl TryFrom<&str> for IssuerUrl {
    type Error = ConfigError;

    /// Admits an https URL with a host and no user, query or fragment.
    fn try_from(text: &str) -> Result<Self, ConfigError> {
        let url = Url::parse(text).map_err(|_| ConfigError::Issuer)?;
        if url.scheme() != "https" || !plain(&url) || url.query().is_some() {
            return Err(ConfigError::Issuer);
        }
        Ok(Self {
            text: text.to_owned(),
            url,
            loopback_http: false,
        })
    }
}

impl IssuerUrl {
    /// Admits an http URL on a loopback address, such as a fake provider's
    /// in a test. The provider's endpoints may then use http on a loopback
    /// address too.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Issuer`] for any other URL.
    #[cfg(feature = "testing")]
    pub fn loopback_http(text: &str) -> Result<Self, ConfigError> {
        let url = Url::parse(text).map_err(|_| ConfigError::Issuer)?;
        if url.scheme() != "http" || !is_loopback(&url) || !plain(&url) || url.query().is_some() {
            return Err(ConfigError::Issuer);
        }
        Ok(Self {
            text: text.to_owned(),
            url,
            loopback_http: true,
        })
    }

    /// The issuer as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The URL of the provider's discovery document.
    pub(crate) fn discovery_url(&self) -> Result<Url, url::ParseError> {
        let base = self.url.as_str().trim_end_matches('/');
        Url::parse(&format!("{base}/.well-known/openid-configuration"))
    }

    /// Whether the provider may serve an endpoint at `url`: https always,
    /// and http on a loopback address only for a loopback test issuer.
    pub(crate) fn admits_endpoint(&self, url: &Url) -> bool {
        url.scheme() == "https"
            || (self.loopback_http && url.scheme() == "http" && is_loopback(url))
    }
}

/// The client ID that the provider issued.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientId(String);

impl TryFrom<&str> for ClientId {
    type Error = ConfigError;

    /// Admits 1 to 256 visible ASCII characters.
    fn try_from(text: &str) -> Result<Self, ConfigError> {
        let visible = text.bytes().all(|byte| byte.is_ascii_graphic());
        if text.is_empty() || text.len() > 256 || !visible {
            return Err(ConfigError::ClientId);
        }
        Ok(Self(text.to_owned()))
    }
}

impl ClientId {
    /// The client ID.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where the provider sends the browser back, such as
/// `http://localhost:8740/auth/oidc/callback`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectUri(Url);

impl TryFrom<&str> for RedirectUri {
    type Error = ConfigError;

    /// Admits an https URL, or an http URL on a loopback host, with no user
    /// or fragment.
    fn try_from(text: &str) -> Result<Self, ConfigError> {
        let url = Url::parse(text).map_err(|_| ConfigError::RedirectUri)?;
        let scheme = url.scheme() == "https" || (url.scheme() == "http" && is_loopback(&url));
        if !scheme || !plain(&url) {
            return Err(ConfigError::RedirectUri);
        }
        Ok(Self(url))
    }
}

impl RedirectUri {
    /// The redirect URI.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// The deadlines of the requests to the provider.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Deadlines {
    pub(crate) connect: Duration,
    pub(crate) total: Duration,
}

/// Everything the authenticator needs except the client secret.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct OidcConfig {
    pub(crate) issuer: IssuerUrl,
    pub(crate) client_id: ClientId,
    pub(crate) redirect_uri: RedirectUri,
    pub(crate) admit: Admit,
    pub(crate) session_lifetime: Duration,
    pub(crate) deadlines: Deadlines,
}

impl OidcConfig {
    /// A config with sessions that last 12 hours and requests to the
    /// provider with a 5 second connect deadline and a 10 second total
    /// deadline.
    #[must_use]
    pub fn new(
        issuer: IssuerUrl,
        client_id: ClientId,
        redirect_uri: RedirectUri,
        admit: Admit,
    ) -> Self {
        Self {
            issuer,
            client_id,
            redirect_uri,
            admit,
            session_lifetime: Duration::from_secs(12 * 60 * 60),
            deadlines: Deadlines {
                connect: Duration::from_secs(5),
                total: Duration::from_secs(10),
            },
        }
    }

    /// Sets how long a session lasts from its sign-in.
    #[must_use]
    pub fn with_session_lifetime(mut self, lifetime: Duration) -> Self {
        self.session_lifetime = lifetime;
        self
    }

    /// Sets both request deadlines to `deadline`, so a test of a stalled
    /// provider ends sooner.
    #[cfg(feature = "testing")]
    #[must_use]
    pub fn with_request_deadline(mut self, deadline: Duration) -> Self {
        self.deadlines = Deadlines {
            connect: deadline,
            total: deadline,
        };
        self
    }
}

#[cfg(test)]
mod tests;

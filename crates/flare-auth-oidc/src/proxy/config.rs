//! What the proxy sign-in is configured with: the issuer and audience its
//! tokens must name, where its key set is, the cookie the proxy sets, the
//! algorithms it may sign with, the claim that names the person, who is
//! admitted, and the proxy's sign-in and sign-out addresses.

pub use super::algorithms::{ProxyAlgorithm, ProxyAlgorithms};
use crate::allow::Admit;
use crate::config::Deadlines;
use crate::identity::Identity;
use std::time::Duration;
use url::Url;

/// Why a proxy config value was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ProxyConfigError {
    /// The issuer is empty, longer than 512 bytes or holds a character
    /// outside visible ASCII.
    #[error("the issuer must be 1 to 512 visible ASCII characters")]
    Issuer,
    /// The audience is empty, longer than 512 bytes or holds a character
    /// outside visible ASCII.
    #[error("the audience must be 1 to 512 visible ASCII characters")]
    Audience,
    /// The key set URL is not an https URL with a host and no user or
    /// fragment.
    #[error("the key set URL must be https, with a host and no user or fragment")]
    KeySetUrl,
    /// The cookie name is empty, longer than 128 bytes or holds a character
    /// that a cookie name cannot hold.
    #[error(
        "the cookie name must be 1 to 128 visible ASCII characters with no separator such as = ; , or a space"
    )]
    Cookie,
    /// The identity claim is not 1 to 64 letters, digits, dots, hyphens,
    /// underscores or colons.
    #[error(
        "the identity claim must be 1 to 64 letters, digits, dots, hyphens, underscores or colons"
    )]
    IdentityClaim,
    /// An algorithm is neither RS256 nor ES256, or none is listed.
    #[error("the algorithms must list RS256, ES256 or both, and nothing else")]
    Algorithm,
    /// A sign-in or sign-out address is not an https URL with a host.
    #[error("a sign-in or sign-out address must be an https URL with a host")]
    Link,
}

/// Whether `text` is 1 to `max` visible ASCII characters.
fn visible(text: &str, max: usize) -> bool {
    (1..=max).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_graphic())
}

/// The issuer that a token's `iss` must equal exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyIssuer(String);

impl TryFrom<&str> for ProxyIssuer {
    type Error = ProxyConfigError;

    /// Admits 1 to 512 visible ASCII characters.
    fn try_from(text: &str) -> Result<Self, ProxyConfigError> {
        visible(text, 512)
            .then(|| Self(text.to_owned()))
            .ok_or(ProxyConfigError::Issuer)
    }
}

impl ProxyIssuer {
    /// The issuer as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The audience that a token's `aud` must contain: flare's application
/// behind the proxy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audience(String);

impl TryFrom<&str> for Audience {
    type Error = ProxyConfigError;

    /// Admits 1 to 512 visible ASCII characters.
    fn try_from(text: &str) -> Result<Self, ProxyConfigError> {
        visible(text, 512)
            .then(|| Self(text.to_owned()))
            .ok_or(ProxyConfigError::Audience)
    }
}

impl Audience {
    /// The audience as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where the proxy publishes its key set, fetched over https.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySetUrl(Url);

fn plain(url: &Url) -> bool {
    url.has_host()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}

impl TryFrom<&str> for KeySetUrl {
    type Error = ProxyConfigError;

    /// Admits an https URL with a host and no user or fragment.
    fn try_from(text: &str) -> Result<Self, ProxyConfigError> {
        let url = Url::parse(text).map_err(|_| ProxyConfigError::KeySetUrl)?;
        if url.scheme() != "https" || !plain(&url) {
            return Err(ProxyConfigError::KeySetUrl);
        }
        Ok(Self(url))
    }
}

impl KeySetUrl {
    /// Admits an http URL on 127.0.0.1, such as a fake issuer's in a test.
    ///
    /// # Errors
    ///
    /// Returns [`ProxyConfigError::KeySetUrl`] for any other URL.
    #[cfg(feature = "testing")]
    pub fn loopback_http(text: &str) -> Result<Self, ProxyConfigError> {
        let url = Url::parse(text).map_err(|_| ProxyConfigError::KeySetUrl)?;
        let loopback = url.host() == Some(url::Host::Ipv4(std::net::Ipv4Addr::LOCALHOST));
        if url.scheme() != "http" || !loopback || !plain(&url) {
            return Err(ProxyConfigError::KeySetUrl);
        }
        Ok(Self(url))
    }

    /// The URL.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub(crate) fn url(&self) -> &Url {
        &self.0
    }
}

/// The name of the cookie in which the proxy sets its signed token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieName(String);

impl TryFrom<&str> for CookieName {
    type Error = ProxyConfigError;

    /// Admits 1 to 128 visible ASCII characters with none of the
    /// separators that RFC 6265 keeps out of a cookie name.
    fn try_from(text: &str) -> Result<Self, ProxyConfigError> {
        let separator = |byte: u8| b"()<>@,;:\\\"/[]?={}".contains(&byte);
        let token = visible(text, 128) && !text.bytes().any(separator);
        token
            .then(|| Self(text.to_owned()))
            .ok_or(ProxyConfigError::Cookie)
    }
}

impl CookieName {
    /// The cookie's name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The claim whose value is the person's email address. It is `email`
/// unless the config names another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityClaim(String);

impl Default for IdentityClaim {
    fn default() -> Self {
        Self("email".to_owned())
    }
}

impl TryFrom<&str> for IdentityClaim {
    type Error = ProxyConfigError;

    /// Admits 1 to 64 letters, digits, dots, hyphens, underscores or colons.
    fn try_from(text: &str) -> Result<Self, ProxyConfigError> {
        let allowed = |byte: u8| byte.is_ascii_alphanumeric() || b".-_:".contains(&byte);
        let name = (1..=64).contains(&text.len()) && text.bytes().all(allowed);
        name.then(|| Self(text.to_owned()))
            .ok_or(ProxyConfigError::IdentityClaim)
    }
}

impl IdentityClaim {
    /// The claim's name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An https address at the proxy, such as its sign-in page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyLink(Url);

impl TryFrom<&str> for ProxyLink {
    type Error = ProxyConfigError;

    /// Admits an https URL with a host and no user.
    fn try_from(text: &str) -> Result<Self, ProxyConfigError> {
        let url = Url::parse(text).map_err(|_| ProxyConfigError::Link)?;
        let userless = url.username().is_empty() && url.password().is_none();
        if url.scheme() != "https" || !url.has_host() || !userless {
            return Err(ProxyConfigError::Link);
        }
        Ok(Self(url))
    }
}

impl ProxyLink {
    /// The address.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Everything the proxy sign-in needs. It holds no secret.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ProxyConfig {
    pub(crate) issuer: ProxyIssuer,
    pub(crate) audience: Audience,
    pub(crate) jwks_url: KeySetUrl,
    pub(crate) cookie: CookieName,
    pub(crate) algorithms: ProxyAlgorithms,
    pub(crate) identity_claim: IdentityClaim,
    pub(crate) identity: Identity,
    pub(crate) admit: Admit,
    pub(crate) sign_in_url: Option<ProxyLink>,
    pub(crate) sign_out_url: Option<ProxyLink>,
    pub(crate) deadlines: Deadlines,
}

impl ProxyConfig {
    /// A config whose identity claim is `email`, with no sign-in or
    /// sign-out address, and requests for the key set with a 5 second
    /// connect deadline and a 10 second total deadline.
    #[must_use]
    pub fn new(
        issuer: ProxyIssuer,
        audience: Audience,
        jwks_url: KeySetUrl,
        cookie: CookieName,
        algorithms: ProxyAlgorithms,
        admit: Admit,
    ) -> Self {
        Self {
            issuer,
            audience,
            jwks_url,
            cookie,
            algorithms,
            identity_claim: IdentityClaim::default(),
            identity: Identity::Email,
            admit,
            sign_in_url: None,
            sign_out_url: None,
            deadlines: Deadlines {
                connect: Duration::from_secs(5),
                total: Duration::from_secs(10),
            },
        }
    }

    /// Sets the algorithms a token may be signed with.
    #[must_use]
    pub fn with_algorithms(mut self, algorithms: ProxyAlgorithms) -> Self {
        self.algorithms = algorithms;
        self
    }

    /// Sets the claim whose value is the person's address.
    #[must_use]
    pub fn with_identity_claim(mut self, claim: IdentityClaim) -> Self {
        self.identity_claim = claim;
        self
    }

    /// Sets how a token names the person. Under [`Identity::Username`] the
    /// token's username claim names the person, `email_verified` is not
    /// required, and the token's `email` is kept only when it is marked
    /// verified. The identity claim then goes unread.
    #[must_use]
    pub fn with_identity(mut self, identity: Identity) -> Self {
        self.identity = identity;
        self
    }

    /// Sets the proxy's sign-in page, which a reader with no cookie is
    /// linked to.
    #[must_use]
    pub fn with_sign_in_url(mut self, link: ProxyLink) -> Self {
        self.sign_in_url = Some(link);
        self
    }

    /// Sets the proxy's sign-out address.
    #[must_use]
    pub fn with_sign_out_url(mut self, link: ProxyLink) -> Self {
        self.sign_out_url = Some(link);
        self
    }

    /// Who is admitted.
    #[must_use]
    pub fn admit(&self) -> &Admit {
        &self.admit
    }

    /// How a token names the person.
    #[must_use]
    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// The cookie in which the proxy sets its token.
    #[must_use]
    pub fn cookie(&self) -> &CookieName {
        &self.cookie
    }

    /// Where the key set is.
    #[must_use]
    pub fn jwks_url(&self) -> &KeySetUrl {
        &self.jwks_url
    }

    /// The proxy's sign-in page, when configured.
    #[must_use]
    pub fn sign_in_url(&self) -> Option<&ProxyLink> {
        self.sign_in_url.as_ref()
    }

    /// The proxy's sign-out address, when configured.
    #[must_use]
    pub fn sign_out_url(&self) -> Option<&ProxyLink> {
        self.sign_out_url.as_ref()
    }
}

#[cfg(test)]
mod tests;

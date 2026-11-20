//! [`Site`]: the one choice, made at start, between the local site on a
//! person's own machine and the hosted site behind a TLS proxy, and
//! [`PublicOrigin`], the hosted site's one origin.
//!
//! The site decides how a request is admitted to the route table, which
//! authenticators exist, which routes are served, which cookies are set,
//! and whether the server writes a server file and exits when idle.

use crate::auth::{Admitted, Gate, Side};
use hangar_auth_loopback::StateDir;
use hangar_auth_oidc::{Clock as OidcClock, OidcAuthenticator, SystemClock as OidcSystemClock};
use hangar_core::{Access, AuthError, Authenticator, Credential, Decision, Resource};
use std::fmt;
use url::Url;

/// Which site a server is, chosen once when it starts.
pub enum Site<K: OidcClock = OidcSystemClock> {
    /// The server on a person's own machine: the launch host on a loopback
    /// address, with the loopback authenticator and, when configured, the
    /// OpenID Connect one, a server file and openers in `state`, and an
    /// idle exit.
    Local {
        /// The authenticators.
        gate: Gate<K>,
        /// Where the server file and openers live.
        state: StateDir,
    },
    /// The server behind a TLS-terminating proxy: one plain-HTTP listener
    /// that serves `origin` only, signs people in with `oidc` alone, sets
    /// only `__Host-` cookies, writes no file and never exits by itself.
    Hosted {
        /// The one origin the site serves.
        origin: PublicOrigin,
        /// The OpenID Connect authenticator.
        oidc: OidcAuthenticator<K>,
    },
}

/// Which site, without what it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SiteKind {
    Local,
    Hosted,
}

impl<K: OidcClock> Site<K> {
    pub(crate) fn kind(&self) -> SiteKind {
        match self {
            Self::Local { .. } => SiteKind::Local,
            Self::Hosted { .. } => SiteKind::Hosted,
        }
    }

    /// The local site's authenticators.
    pub(crate) fn gate(&self) -> Option<&Gate<K>> {
        match self {
            Self::Local { gate, .. } => Some(gate),
            Self::Hosted { .. } => None,
        }
    }

    /// The local site's state directory.
    pub(crate) fn state(&self) -> Option<&StateDir> {
        match self {
            Self::Local { state, .. } => Some(state),
            Self::Hosted { .. } => None,
        }
    }

    /// The OpenID Connect authenticator, when the site has one.
    pub(crate) fn oidc(&self) -> Option<&OidcAuthenticator<K>> {
        match self {
            Self::Local { gate, .. } => gate.oidc(),
            Self::Hosted { oidc, .. } => Some(oidc),
        }
    }

    /// The hosted site's origin.
    pub(crate) fn origin(&self) -> Option<&PublicOrigin> {
        match self {
            Self::Local { .. } => None,
            Self::Hosted { origin, .. } => Some(origin),
        }
    }

    /// Asks the site's authenticators who `credential` names: the gate on
    /// the local site, and the OpenID Connect authenticator alone on the
    /// hosted site.
    pub(crate) async fn authenticate(
        &self,
        credential: &Credential,
    ) -> Result<Admitted, AuthError> {
        match self {
            Self::Local { gate, .. } => gate.authenticate(credential).await,
            Self::Hosted { oidc, .. } => Ok(Admitted {
                principal: oidc.authenticate(credential).await?,
                by: Side::Oidc,
            }),
        }
    }

    /// Asks the side that admitted the request.
    pub(crate) async fn authorize(
        &self,
        admitted: &Admitted,
        access: Access,
        resource: &Resource,
    ) -> Result<Decision, AuthError> {
        match (self, admitted.by) {
            (Self::Local { gate, .. }, _) => gate.authorize(admitted, access, resource).await,
            (Self::Hosted { oidc, .. }, Side::Oidc) => {
                oidc.authorize(&admitted.principal, access, resource).await
            }
            (Self::Hosted { .. }, _) => Err(AuthError::Rejected),
        }
    }
}

impl<K: OidcClock> fmt::Debug for Site<K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local { gate, .. } => formatter
                .debug_struct("Local")
                .field("gate", gate)
                .finish_non_exhaustive(),
            Self::Hosted { origin, oidc } => formatter
                .debug_struct("Hosted")
                .field("origin", origin)
                .field("oidc", oidc)
                .finish(),
        }
    }
}

/// Why a public origin was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum OriginError {
    /// The text is not an https URL with a host and nothing after it.
    #[error(
        "the origin must be https://<host> or https://<host>:<port>, with no user, path, query or fragment, not even a trailing slash"
    )]
    NotAnOrigin,
}

/// The one origin a hosted site serves, such as `https://hangar.example.com`.
/// It is built with `url::Url`, so the host is lower-cased, an
/// internationalised name is in its ASCII form and port 443 is dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicOrigin {
    serialized: String,
    authority: String,
}

impl TryFrom<&str> for PublicOrigin {
    type Error = OriginError;

    /// Admits `https://<host>` or `https://<host>:<port>` and nothing more.
    fn try_from(text: &str) -> Result<Self, OriginError> {
        Self::parse(text, "https")
    }
}

impl PublicOrigin {
    fn parse(text: &str, scheme: &str) -> Result<Self, OriginError> {
        let refused = OriginError::NotAnOrigin;
        let (_, rest) = text.split_once("://").ok_or(refused)?;
        if rest.is_empty() || rest.contains(['/', '?', '#', '\\', '@']) {
            return Err(refused);
        }
        let url = Url::parse(text).map_err(|_| refused)?;
        let plain = url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/";
        if url.scheme() != scheme || !plain {
            return Err(refused);
        }
        let host = url
            .host_str()
            .filter(|host| !host.is_empty())
            .ok_or(refused)?;
        let authority = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_owned(),
        };
        Ok(Self {
            serialized: format!("{scheme}://{authority}"),
            authority,
        })
    }

    /// Admits `http://localhost:<port>`, for tests that serve a hosted site
    /// without TLS. `hangar serve` never enables the `testing` feature, and
    /// no config key reaches this.
    ///
    /// # Errors
    ///
    /// Returns [`OriginError::NotAnOrigin`] for any other text.
    #[cfg(feature = "testing")]
    pub fn loopback_http(text: &str) -> Result<Self, OriginError> {
        let origin = Self::parse(text, "http")?;
        let host = Url::parse(text).map_err(|_| OriginError::NotAnOrigin)?;
        match host.host() {
            Some(url::Host::Domain("localhost")) => Ok(origin),
            _ => Err(OriginError::NotAnOrigin),
        }
    }

    /// The origin, such as `https://hangar.example.com`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.serialized
    }

    /// What a request's Host header must be: the host, and the port when
    /// it is not the scheme's default.
    #[must_use]
    pub fn authority(&self) -> &str {
        &self.authority
    }

    /// The redirect URI to register with the provider:
    /// `<origin>/auth/oidc/callback`.
    #[must_use]
    pub fn redirect_uri(&self) -> String {
        format!("{}/auth/oidc/callback", self.serialized)
    }
}

#[cfg(test)]
mod tests;

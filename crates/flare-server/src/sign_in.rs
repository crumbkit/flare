//! [`HostedSignIn`]: what a hosted site needs from its sign-in beyond
//! [`Authenticator`]: the cookie that carries a browser's credential, what
//! a page request with no credential gets, where sign-out goes, and why a
//! credential was refused. flare's OpenID Connect sign-in and the proxy
//! sign-in implement it, and an app that embeds the server can pass its
//! own.

use crate::secret::HOSTED_SESSION_COOKIE;
use flare_auth_oidc::{
    Clock as OidcClock, OidcAuthenticator, ProxyAuthenticator, ProxyRefusal,
    SystemClock as OidcSystemClock,
};
use flare_core::{AuthError, Authenticator, Credential, Principal};
use std::fmt;
use std::future::Future;

/// What a page request with no credential gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unsigned<'a> {
    /// A 401 page that links to flare's own `/auth/oidc/start`.
    OidcStart,
    /// A 401 page that links to this sign-in address, or has no link.
    SignInAt(Option<&'a str>),
}

/// Why a hosted sign-in refused a credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HostedRefusal {
    /// The credential names no one: a 401 that offers the sign-in.
    SignInNeeded,
    /// The credential failed the named check: a 401 with no sign-in link
    /// or redirect, so a misconfigured check cannot loop the browser.
    Refused {
        /// The check, such as `audience`, for the log.
        check: &'static str,
    },
    /// The credential names a person the admit rule does not admit: a 403
    /// that links to the sign-out address, if any.
    NotAdmitted,
    /// The sign-in cannot decide now: a 503.
    Unavailable,
}

/// The sign-in of a hosted site. `Site::Hosted` holds one, and
/// [`OidcAuthenticator`] is its default type.
pub trait HostedSignIn<K: OidcClock = OidcSystemClock>:
    Authenticator + fmt::Debug + 'static
{
    /// The name of the cookie that carries a browser's credential.
    fn cookie(&self) -> &str;

    /// What a page request with no credential gets.
    fn unsigned(&self) -> Unsigned<'_>;

    /// Where sign-out goes: an address that a refused person follows to
    /// sign in as someone else, or `None`.
    fn sign_out(&self) -> Option<&str>;

    /// Identifies the principal that `credential` proves, and says why it
    /// was refused. The default reads [`Authenticator::authenticate`]: a
    /// rejected credential needs a sign-in.
    fn sign_in(
        &self,
        credential: &Credential,
    ) -> impl Future<Output = Result<Principal, HostedRefusal>> + Send {
        async move {
            self.authenticate(credential)
                .await
                .map_err(|error| match error {
                    AuthError::Unavailable => HostedRefusal::Unavailable,
                    _ => HostedRefusal::SignInNeeded,
                })
        }
    }

    /// flare's OpenID Connect authenticator, when this sign-in is it, so
    /// the `/auth/oidc/*` routes serve. Every other sign-in answers `None`,
    /// and those routes answer 404.
    fn oidc(&self) -> Option<&OidcAuthenticator<K>> {
        None
    }
}

impl<K: OidcClock> HostedSignIn<K> for OidcAuthenticator<K> {
    fn cookie(&self) -> &str {
        HOSTED_SESSION_COOKIE
    }

    fn unsigned(&self) -> Unsigned<'_> {
        Unsigned::OidcStart
    }

    fn sign_out(&self) -> Option<&str> {
        None
    }

    fn oidc(&self) -> Option<&OidcAuthenticator<K>> {
        Some(self)
    }
}

impl<K: OidcClock, C: OidcClock> HostedSignIn<K> for ProxyAuthenticator<C> {
    fn cookie(&self) -> &str {
        self.config().cookie().as_str()
    }

    fn unsigned(&self) -> Unsigned<'_> {
        Unsigned::SignInAt(self.config().sign_in_url().map(|link| link.as_str()))
    }

    fn sign_out(&self) -> Option<&str> {
        self.config().sign_out_url().map(|link| link.as_str())
    }

    async fn sign_in(&self, credential: &Credential) -> Result<Principal, HostedRefusal> {
        self.check(credential)
            .await
            .map_err(|refusal| match refusal {
                ProxyRefusal::NotAdmitted => HostedRefusal::NotAdmitted,
                refusal if refusal.is_temporary() => HostedRefusal::Unavailable,
                refusal => HostedRefusal::Refused {
                    check: refusal.check(),
                },
            })
    }
}

//! [`Gate`]: the loopback authenticator and, when configured, the OpenID
//! Connect one, and the rule that `authorize` goes to the side that
//! authenticated.

use flare_auth_loopback::Loopback;
use flare_auth_oidc::{Clock as OidcClock, OidcAuthenticator, SystemClock as OidcSystemClock};
use flare_core::{Access, AuthError, Authenticator, Credential, Decision, Principal, Resource};
use std::fmt;

/// Which authenticator accepted a credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Side {
    /// The loopback authenticator: the launch token or a loopback session.
    Loopback,
    /// The OpenID Connect authenticator: one of its sessions.
    Oidc,
}

/// A credential that one side accepted, for one request. Nothing keeps it
/// past the request.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Admitted {
    /// The principal the side named.
    pub principal: Principal,
    /// The side that accepted the credential.
    pub by: Side,
}

/// Both authenticators. `Gate` does not implement `Authenticator`, because
/// that trait's `authorize` takes a bare principal and so cannot say which
/// side accepted it.
pub struct Gate<C: OidcClock = OidcSystemClock> {
    loopback: Loopback,
    oidc: Option<OidcAuthenticator<C>>,
}

impl<C: OidcClock> Gate<C> {
    /// A gate with the loopback authenticator and, when given, the OpenID
    /// Connect one.
    #[must_use]
    pub fn new(loopback: Loopback, oidc: Option<OidcAuthenticator<C>>) -> Self {
        Self { loopback, oidc }
    }

    /// The loopback authenticator.
    #[must_use]
    pub fn loopback(&self) -> &Loopback {
        &self.loopback
    }

    /// The OpenID Connect authenticator, when configured.
    #[must_use]
    pub fn oidc(&self) -> Option<&OidcAuthenticator<C>> {
        self.oidc.as_ref()
    }

    /// Asks the loopback side first, and the OpenID Connect side only when
    /// loopback rejects the credential.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Rejected`] when neither side accepts the
    /// credential and [`AuthError::Unavailable`] when the side that would
    /// decide cannot answer.
    pub async fn authenticate(&self, credential: &Credential) -> Result<Admitted, AuthError> {
        match self.loopback.authenticate(credential).await {
            Ok(principal) => {
                return Ok(Admitted {
                    principal,
                    by: Side::Loopback,
                });
            }
            Err(AuthError::Rejected) => {}
            Err(other) => return Err(other),
        }
        let oidc = self.oidc.as_ref().ok_or(AuthError::Rejected)?;
        let principal = oidc.authenticate(credential).await?;
        Ok(Admitted {
            principal,
            by: Side::Oidc,
        })
    }

    /// Asks only the side that accepted the credential.
    ///
    /// # Errors
    ///
    /// Returns the side's [`AuthError`]; [`AuthError::Unavailable`] when the
    /// OpenID Connect side accepted and is no longer configured, which
    /// cannot happen within one gate.
    pub async fn authorize(
        &self,
        admitted: &Admitted,
        access: Access,
        resource: &Resource,
    ) -> Result<Decision, AuthError> {
        match admitted.by {
            Side::Loopback => {
                self.loopback
                    .authorize(&admitted.principal, access, resource)
                    .await
            }
            Side::Oidc => {
                let oidc = self.oidc.as_ref().ok_or(AuthError::Unavailable)?;
                oidc.authorize(&admitted.principal, access, resource).await
            }
        }
    }
}

impl<C: OidcClock> fmt::Debug for Gate<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Gate")
            .field("loopback", &self.loopback)
            .field("oidc", &self.oidc.is_some())
            .finish()
    }
}

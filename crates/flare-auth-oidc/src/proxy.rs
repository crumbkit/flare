//! [`ProxyAuthenticator`]: trusts the signed token that a single sign-on
//! proxy in front of flare sets as a cookie, and the same token as a
//! bearer. It creates no session and stores nothing: the proxy's token is
//! the session, and the key set lives in memory.

mod algorithms;
mod claims;
mod config;

pub use config::{
    Audience, CookieName, IdentityClaim, KeySetUrl, ProxyAlgorithm, ProxyAlgorithms, ProxyConfig,
    ProxyConfigError, ProxyIssuer, ProxyLink,
};

use crate::clock::{Clock, SystemClock};
use crate::keys::{KeyCache, KeyError, KeySet};
use crate::provider;
use crate::refusal::{ProxyConnectError, ProxyRefusal};
use flare_core::{Access, AuthError, Authenticator, Credential, Decision, Principal, Resource};
use secrecy::ExposeSecret as _;
use std::fmt;

/// Signs people in with the token a single sign-on proxy signed. The crate
/// docs give the checks in order and the key set's states.
pub struct ProxyAuthenticator<C: Clock = SystemClock> {
    config: ProxyConfig,
    client: reqwest::Client,
    keys: KeyCache,
    clock: C,
}

impl ProxyAuthenticator {
    /// Fetches the key set from the configured address and checks it.
    /// Needs a Tokio runtime.
    ///
    /// # Errors
    ///
    /// Returns [`ProxyConnectError::Unreachable`] when the key set cannot
    /// be fetched, and [`ProxyConnectError::Unusable`] when it holds no
    /// usable key or more than 20 keys.
    pub async fn connect(config: ProxyConfig) -> Result<Self, ProxyConnectError> {
        Self::connect_with_clock(config, SystemClock).await
    }
}

impl<C: Clock> ProxyAuthenticator<C> {
    /// [`ProxyAuthenticator::connect`] with a clock of the caller's.
    ///
    /// # Errors
    ///
    /// As [`ProxyAuthenticator::connect`].
    pub async fn connect_with_clock(
        config: ProxyConfig,
        clock: C,
    ) -> Result<Self, ProxyConnectError> {
        let client =
            provider::client(config.deadlines).map_err(|_| ProxyConnectError::Unreachable)?;
        let body = provider::fetch(&client, config.jwks_url.url().clone())
            .await
            .map_err(|_| ProxyConnectError::Unreachable)?;
        let rule = config.algorithms.key_rule();
        let keys = KeySet::parse_under(&body, rule).ok_or(ProxyConnectError::Unusable)?;
        Ok(Self {
            keys: KeyCache::under(keys, clock.now(), rule),
            config,
            client,
            clock,
        })
    }

    /// The config, for the server that hosts the sign-in: its cookie, its
    /// sign-in and sign-out addresses and its admit rule.
    #[must_use]
    pub fn config(&self) -> &ProxyConfig {
        &self.config
    }

    /// Checks the token that `credential` carries, a cookie's or a
    /// bearer's alike, and returns the principal the admit rule gives its
    /// address. Needs a Tokio runtime when the key set must be fetched.
    ///
    /// # Errors
    ///
    /// Returns the [`ProxyRefusal`] of the first check that fails, in the
    /// order the crate docs give.
    pub async fn check(&self, credential: &Credential) -> Result<Principal, ProxyRefusal> {
        let secret = match credential {
            Credential::Session(secret) | Credential::Bearer(secret) => secret,
            _ => return Err(ProxyRefusal::Malformed),
        };
        let token = secret.expose_secret();
        let (algorithm, kid) = claims::header(token, &self.config)?;
        let now = self.clock.now();
        let url = self.config.jwks_url.url();
        let key = self
            .keys
            .key_with(&kid, now, || provider::fetch(&self.client, url.clone()))
            .await
            .map_err(|error| match error {
                KeyError::Unknown => ProxyRefusal::UnknownKey,
                KeyError::Unavailable | KeyError::Unusable => ProxyRefusal::KeysUnavailable,
            })?;
        claims::verify(token, &key, algorithm, &self.config, now)
    }
}

impl<C: Clock> Authenticator for ProxyAuthenticator<C> {
    /// Accepts a session or bearer credential whose token passes every
    /// check. A key set that cannot be fetched is
    /// [`AuthError::Unavailable`]; every other refusal is
    /// [`AuthError::Rejected`].
    async fn authenticate(&self, credential: &Credential) -> Result<Principal, AuthError> {
        self.check(credential).await.map_err(|refusal| {
            if refusal.is_temporary() {
                AuthError::Unavailable
            } else {
                AuthError::Rejected
            }
        })
    }

    /// Allows a principal the resources the admit rule's reach names, for
    /// read and for write, and denies everything else. Makes no request.
    async fn authorize(
        &self,
        principal: &Principal,
        _access: Access,
        resource: &Resource,
    ) -> Result<Decision, AuthError> {
        Ok(if self.config.admit.address_reaches(principal, resource) {
            Decision::Allowed
        } else {
            Decision::Denied
        })
    }
}

impl<C: Clock> fmt::Debug for ProxyAuthenticator<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProxyAuthenticator")
            .field("issuer", &self.config.issuer.as_str())
            .field("audience", &self.config.audience.as_str())
            .field("keys", &self.keys)
            .finish_non_exhaustive()
    }
}

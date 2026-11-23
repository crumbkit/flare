//! [`OidcAuthenticator`]: connect, start and complete a sign-in, sign out,
//! and the [`Authenticator`] implementation.

use crate::allow::Admit;
use crate::clock::{Clock, SystemClock};
use crate::config::OidcConfig;
use crate::flow::{Callback, KeptState, Outcome, ReturnTo, SignInStart, begin};
use crate::keys::{KeyCache, KeyError, KeySet};
use crate::provider::Provider;
use crate::refusal::{ConnectError, SignInRefusal, TokenFault};
use crate::seal::Sealer;
use crate::secret::ClientSecretSource;
use crate::session::{Sessions, SignedIn};
use crate::stores::{DynKeys, DynSessions};
use crate::token::{self, Expected};
use flare_core::memory::{MemorySealingKeys, MemorySessionStore};
use flare_core::{
    Access, AuthError, Authenticator, Credential, Decision, Principal, Resource, SealingKeys,
    SessionStore,
};
use secrecy::SecretString;
use std::fmt;
use std::sync::Arc;

/// Signs people in with an OpenID Connect provider and authenticates the
/// sessions it creates. The crate docs give its state tables.
pub struct OidcAuthenticator<C: Clock = SystemClock> {
    config: OidcConfig,
    secret: SecretString,
    provider: Provider,
    keys: KeyCache,
    sealer: Sealer,
    sessions: Sessions,
    clock: C,
}

impl OidcAuthenticator {
    /// Reads the client secret, draws the key that seals pending sign-ins,
    /// fetches the provider's discovery document and checks it, then
    /// fetches the key set and checks it. Needs a Tokio runtime.
    ///
    /// # Errors
    ///
    /// Returns a [`ConnectError`] for the first step that fails. The secret
    /// is read and the key drawn before any request.
    pub async fn connect(
        config: OidcConfig,
        secret: &impl ClientSecretSource,
    ) -> Result<Self, ConnectError> {
        Self::connect_with_clock(config, secret, SystemClock).await
    }

    /// [`OidcAuthenticator::connect`] with stores for the sessions and the
    /// sealing keys, which a hosted site that runs on more than one process
    /// shares between its processes, such as the DynamoDB stores of
    /// `flare-aws`.
    ///
    /// # Errors
    ///
    /// As [`OidcAuthenticator::connect`].
    pub async fn connect_stored(
        config: OidcConfig,
        secret: &impl ClientSecretSource,
        sessions: impl SessionStore + 'static,
        keys: impl SealingKeys + 'static,
    ) -> Result<Self, ConnectError> {
        Self::connect_with_stores(config, secret, SystemClock, sessions, keys).await
    }
}

impl<C: Clock> OidcAuthenticator<C> {
    /// [`OidcAuthenticator::connect`] with a clock of the caller's.
    ///
    /// # Errors
    ///
    /// As [`OidcAuthenticator::connect`].
    pub async fn connect_with_clock(
        config: OidcConfig,
        secret: &impl ClientSecretSource,
        clock: C,
    ) -> Result<Self, ConnectError> {
        Self::connect_with_stores(
            config,
            secret,
            clock,
            MemorySessionStore::new(),
            MemorySealingKeys::new(),
        )
        .await
    }

    /// [`OidcAuthenticator::connect`] with a clock of the caller's and
    /// stores for the sessions and the sealing keys. A hosted site that
    /// runs on more than one process passes stores that every process
    /// shares, such as the DynamoDB stores of `flare-aws`.
    ///
    /// # Errors
    ///
    /// As [`OidcAuthenticator::connect`].
    pub async fn connect_with_stores(
        config: OidcConfig,
        secret: &impl ClientSecretSource,
        clock: C,
        sessions: impl SessionStore + 'static,
        keys: impl SealingKeys + 'static,
    ) -> Result<Self, ConnectError> {
        let secret = secret.client_secret().map_err(ConnectError::Secret)?;
        let keys: Arc<dyn DynKeys> = Arc::new(keys);
        let sealer = Sealer::new(keys, &config.client_id);
        let sessions = Sessions::new(
            Arc::new(sessions) as Arc<dyn DynSessions>,
            config.session_lifetime,
        );
        let provider = Provider::discover(&config.issuer, config.deadlines).await?;
        let body = provider
            .fetch_keys()
            .await
            .map_err(|_| ConnectError::Keys)?;
        let keys = KeySet::parse(&body).ok_or(ConnectError::Keys)?;
        Ok(Self {
            keys: KeyCache::new(keys, clock.now()),
            config,
            secret,
            provider,
            sealer,
            sessions,
            clock,
        })
    }

    /// Starts a sign-in, sealed into the kept value it returns, which
    /// opens for 10 minutes. Makes no request and stores nothing.
    ///
    /// # Errors
    ///
    /// Returns [`SignInRefusal::NoRandomness`] when the system's random
    /// number generator fails, and [`SignInRefusal::CannotSeal`] when the
    /// sign-in cannot be sealed.
    pub async fn start(&self, return_to: ReturnTo) -> Result<SignInStart, SignInRefusal> {
        begin(
            &self.provider.endpoints.authorization,
            &self.config,
            &self.sealer,
            return_to,
            self.clock.now(),
        )
        .await
    }

    /// Completes a sign-in from the provider's callback. `query` is the
    /// callback's raw query string, `kept` is the sealed value the server
    /// kept from [`start`](Self::start), or `None` when the browser sent
    /// none, and `old` is the session credential the browser already holds,
    /// if any. Needs a Tokio runtime.
    ///
    /// The kept value opens again until it is 10 minutes old, so a second
    /// callback with it sends the code to the provider again, and the
    /// provider refuses a used code as [`SignInRefusal::CodeRejected`].
    ///
    /// The session of `old` ends only when the sign-in succeeds, in the
    /// same step that creates the new session. A refused sign-in leaves it
    /// live.
    ///
    /// # Errors
    ///
    /// Returns the [`SignInRefusal`] of the first check that fails, in the
    /// order the crate docs give.
    pub async fn complete(
        &self,
        query: &str,
        kept: Option<&KeptState>,
        old: Option<&Credential>,
    ) -> Result<SignedIn, SignInRefusal> {
        let callback = Callback::parse(query)?;
        let kept = kept.ok_or(SignInRefusal::NoKeptState)?;
        let pending = self.sealer.open(kept, self.clock.now()).await?;
        if !pending.matches(&callback.state) {
            return Err(SignInRefusal::WrongState);
        }
        let Outcome::Code(code) = callback.outcome else {
            return Err(SignInRefusal::ProviderDenied);
        };
        let id_token = token::redeem(
            &self.provider,
            &self.config,
            &self.secret,
            &code,
            &pending.verifier,
        )
        .await?;
        let kid = token::key_id(&id_token).map_err(SignInRefusal::TokenRejected)?;
        let now = self.clock.now();
        let key =
            self.keys
                .key_for(&kid, &self.provider, now)
                .await
                .map_err(|error| match error {
                    KeyError::Unknown => SignInRefusal::TokenRejected(TokenFault::UnknownKey),
                    KeyError::Unavailable => SignInRefusal::ProviderUnavailable,
                    KeyError::Unusable => SignInRefusal::ProviderMisbehaved,
                })?;
        let expected = Expected {
            config: &self.config,
            nonce: &pending.nonce,
            now,
        };
        let verified = token::check(&id_token, &key, &expected)?;
        let principal = self
            .config
            .admit
            .principal(&verified.email, verified.hd.as_ref())?;
        self.sessions
            .create(principal, pending.return_to, self.clock.now(), old)
            .await
    }

    /// Ends the session of `credential`. A credential that names no session
    /// changes nothing.
    ///
    /// # Errors
    ///
    /// Returns [`AuthError::Unavailable`] when the session store cannot
    /// answer, and the session may then still be live.
    pub async fn sign_out(&self, credential: &Credential) -> Result<(), AuthError> {
        match credential {
            Credential::Session(secret) => self.sessions.remove(secret).await,
            _ => Ok(()),
        }
    }
}

impl<C: Clock> Authenticator for OidcAuthenticator<C> {
    /// Accepts a [`Credential::Session`] that names a live session, and
    /// rejects every other credential. Makes no request.
    async fn authenticate(&self, credential: &Credential) -> Result<Principal, AuthError> {
        let Credential::Session(secret) = credential else {
            return Err(AuthError::Rejected);
        };
        self.sessions
            .principal(secret, self.clock.now())
            .await?
            .ok_or(AuthError::Rejected)
    }

    /// Allows a principal the resources its reach names, for read and for
    /// write, and denies everything else. Under [`Admit::Emails`] the
    /// principal must be on the allow-list. Under [`Admit::Domain`] it must
    /// hold a live session of this authenticator, and the rule's reach
    /// applies; [`Admit::Anyone`] is decided the same way. Makes no request.
    async fn authorize(
        &self,
        principal: &Principal,
        _access: Access,
        resource: &Resource,
    ) -> Result<Decision, AuthError> {
        let reaches = match &self.config.admit {
            Admit::Emails(allow) => allow
                .by_principal(principal.id())
                .is_some_and(|entry| entry.reach().reaches(resource)),
            Admit::Domain(rule) => {
                self.sessions
                    .holds(principal.id(), self.clock.now())
                    .await?
                    && rule.reach().reaches(resource)
            }
            Admit::Anyone(reach) => {
                self.sessions
                    .holds(principal.id(), self.clock.now())
                    .await?
                    && reach.reaches(resource)
            }
        };
        Ok(if reaches {
            Decision::Allowed
        } else {
            Decision::Denied
        })
    }
}

impl<C: Clock> fmt::Debug for OidcAuthenticator<C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OidcAuthenticator")
            .field("issuer", &self.config.issuer.as_str())
            .field("client_id", &self.config.client_id.as_str())
            .field("keys", &self.keys)
            .finish_non_exhaustive()
    }
}

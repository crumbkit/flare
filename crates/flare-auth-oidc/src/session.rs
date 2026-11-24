//! Sessions: the store of live sessions, keyed by the hash of their secret,
//! and what a finished sign-in returns.

use crate::clock::unix_seconds;
use crate::flow::{ReturnTo, hash, random_secret};
use crate::refusal::SignInRefusal;
use crate::stores::DynSessions;
use flare_core::{
    AuthError, Credential, Digest, NewSession, Principal, PrincipalId, SessionRule,
    SessionStoreError, Timestamp,
};
use secrecy::{ExposeSecret as _, SecretString};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// What a finished sign-in returns. The session credential appears here
/// once, and the crate keeps only its hash. Its `Debug` output hides the
/// principal, which under [`Admit::Domain`](crate::Admit::Domain) is the
/// person's email.
#[non_exhaustive]
pub struct SignedIn {
    session: Credential,
    secret: SecretString,
    principal: Principal,
    return_to: ReturnTo,
    expires_in: Duration,
    cookie_life: Duration,
}

impl SignedIn {
    /// The session credential, for the server to put in a cookie.
    #[must_use]
    pub fn session(&self) -> &Credential {
        &self.session
    }

    /// The secret of the session credential, for a server that sets it as
    /// a cookie without matching on the credential's kind.
    #[must_use]
    pub fn session_secret(&self) -> &SecretString {
        &self.secret
    }

    /// Who signed in.
    #[must_use]
    pub fn principal(&self) -> &Principal {
        &self.principal
    }

    /// The return-to value that `start` was given.
    #[must_use]
    pub fn return_to(&self) -> &ReturnTo {
        &self.return_to
    }

    /// How long the session lasts from now when no request extends it.
    #[must_use]
    pub fn expires_in(&self) -> Duration {
        self.expires_in
    }

    /// How long the browser keeps the session cookie: the longest the
    /// session can live from sign-in, 7 days. The store, not the cookie,
    /// ends a session that has been idle.
    #[must_use]
    pub fn cookie_life(&self) -> Duration {
        self.cookie_life
    }
}

impl fmt::Debug for SignedIn {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignedIn")
            .field("session", &self.session)
            .field("principal", &"<hidden>")
            .field("return_to", &self.return_to)
            .field("expires_in", &self.expires_in)
            .field("cookie_life", &self.cookie_life)
            .finish()
    }
}

/// The session store and the rule that decides a session's life.
#[derive(Clone)]
pub(crate) struct Sessions {
    store: Arc<dyn DynSessions>,
    rule: SessionRule,
}

fn stamp(time: SystemTime) -> Timestamp {
    Timestamp::from_secs(unix_seconds(time))
}

fn digest(secret: &SecretString) -> Digest {
    Digest::from_bytes(hash(secret.expose_secret()))
}

impl Sessions {
    pub(crate) fn new(store: Arc<dyn DynSessions>, lifetime: Duration) -> Self {
        Self {
            store,
            rule: SessionRule::new(lifetime.as_secs()),
        }
    }

    /// How long a browser keeps the session cookie: the hard cap.
    pub(crate) fn cookie_life(&self) -> Duration {
        Duration::from_secs(self.rule.cap())
    }

    /// Creates a session for `principal` in a browser that holds the
    /// session `old`, if any. It draws the new secret and writes the
    /// session in one step with the end of `old` and, past 8 sessions, of
    /// the person's earliest-expiring one. The credential is returned only
    /// after the write settled.
    pub(crate) async fn create(
        &self,
        principal: Principal,
        return_to: ReturnTo,
        now: SystemTime,
        old: Option<&Credential>,
    ) -> Result<SignedIn, SignInRefusal> {
        let secret = random_secret()?;
        let replacing = match old {
            Some(Credential::Session(old)) => Some(digest(old)),
            _ => None,
        };
        let session = NewSession::new(digest(&secret), principal.clone());
        self.store
            .create(session, replacing, &self.rule, stamp(now))
            .await
            .map_err(|error| match error {
                SessionStoreError::Full => SignInRefusal::TooManySessions,
                _ => SignInRefusal::StoreUnavailable,
            })?;
        Ok(SignedIn {
            session: Credential::Session(secret.clone()),
            secret,
            principal,
            return_to,
            expires_in: Duration::from_secs(self.rule.idle()),
            cookie_life: self.cookie_life(),
        })
    }

    /// The principal of a live session, which the request extends when the
    /// rule says so.
    pub(crate) async fn principal(
        &self,
        secret: &SecretString,
        now: SystemTime,
    ) -> Result<Option<Principal>, AuthError> {
        self.store
            .principal(&digest(secret), &self.rule, stamp(now))
            .await
            .map_err(|_| AuthError::Unavailable)
    }

    /// Whether `person` holds a live session.
    pub(crate) async fn holds(
        &self,
        person: &PrincipalId,
        now: SystemTime,
    ) -> Result<bool, AuthError> {
        self.store
            .holds(person, stamp(now))
            .await
            .map_err(|_| AuthError::Unavailable)
    }

    /// Ends a session.
    pub(crate) async fn remove(&self, secret: &SecretString) -> Result<(), AuthError> {
        self.store
            .remove(&digest(secret))
            .await
            .map_err(|_| AuthError::Unavailable)
    }
}

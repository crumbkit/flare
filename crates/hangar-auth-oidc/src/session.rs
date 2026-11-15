//! Sessions: the table of live sessions, keyed by the hash of their secret,
//! and what a finished sign-in returns.

use crate::flow::{ReturnTo, hash, random_secret};
use crate::refusal::SignInRefusal;
use hangar_core::{Credential, Principal};
use secrecy::{ExposeSecret as _, SecretString};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// How many sessions may be live at once.
const MAX_SESSIONS: usize = 1024;

/// What a finished sign-in returns. The session credential appears here
/// once, and the crate keeps only its hash.
#[derive(Debug)]
#[non_exhaustive]
pub struct SignedIn {
    session: Credential,
    principal: Principal,
    return_to: ReturnTo,
    expires_in: Duration,
}

impl SignedIn {
    /// The session credential, for the server to put in a cookie.
    #[must_use]
    pub fn session(&self) -> &Credential {
        &self.session
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

    /// How long the session lasts from now.
    #[must_use]
    pub fn expires_in(&self) -> Duration {
        self.expires_in
    }
}

#[derive(Debug)]
struct Live {
    principal: Principal,
    expires: SystemTime,
}

/// The live sessions.
#[derive(Debug, Default)]
pub(crate) struct Sessions(HashMap<[u8; 32], Live>);

impl Sessions {
    /// Creates a session for `principal` that lasts `lifetime`. It first
    /// removes expired sessions, and refuses while 1,024 are live.
    pub(crate) fn create(
        &mut self,
        principal: Principal,
        return_to: ReturnTo,
        lifetime: Duration,
        now: SystemTime,
    ) -> Result<SignedIn, SignInRefusal> {
        self.0.retain(|_, live| live.expires > now);
        if self.0.len() >= MAX_SESSIONS {
            return Err(SignInRefusal::TooManySessions);
        }
        let secret = random_secret()?;
        // A lifetime that runs past the end of SystemTime ends the session
        // at once, and no such lifetime is configured in practice.
        let expires = now.checked_add(lifetime).unwrap_or(now);
        self.0.insert(
            hash(secret.expose_secret()),
            Live {
                principal: principal.clone(),
                expires,
            },
        );
        Ok(SignedIn {
            session: Credential::Session(secret),
            principal,
            return_to,
            expires_in: lifetime,
        })
    }

    /// The principal of a live session. An expired session is removed.
    pub(crate) fn principal(
        &mut self,
        secret: &SecretString,
        now: SystemTime,
    ) -> Option<Principal> {
        let key = hash(secret.expose_secret());
        let live = self.0.get(&key)?;
        if live.expires > now {
            return Some(live.principal.clone());
        }
        self.0.remove(&key);
        None
    }

    /// Removes a session.
    pub(crate) fn remove(&mut self, secret: &SecretString) {
        self.0.remove(&hash(secret.expose_secret()));
    }
}

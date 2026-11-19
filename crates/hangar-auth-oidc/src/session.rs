//! Sessions: the table of live sessions, keyed by the hash of their secret,
//! and what a finished sign-in returns.

use crate::flow::{ReturnTo, hash, random_secret};
use crate::refusal::SignInRefusal;
use hangar_core::{Credential, Principal, PrincipalId};
use secrecy::{ExposeSecret as _, SecretString};
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, SystemTime};

/// How many sessions may be live at once.
const MAX_SESSIONS: usize = 1024;
/// How many sessions one principal may hold at once.
const MAX_PER_PRINCIPAL: usize = 8;

/// What a finished sign-in returns. The session credential appears here
/// once, and the crate keeps only its hash. Its `Debug` output hides the
/// principal, which under [`Admit::Domain`](crate::Admit::Domain) is the
/// person's email.
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

impl fmt::Debug for SignedIn {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignedIn")
            .field("session", &self.session)
            .field("principal", &"<hidden>")
            .field("return_to", &self.return_to)
            .field("expires_in", &self.expires_in)
            .finish()
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
    /// Creates a session for `principal` that lasts `lifetime`, in a
    /// browser that holds the session `old`, if any. It draws the new
    /// secret, removes expired sessions, then decides in this order: the
    /// session of `old` goes, then the principal's session with the
    /// earliest expiry while it holds 8, and only then does a table still
    /// holding 1,024 sessions refuse. A refusal removes nothing that is
    /// live.
    pub(crate) fn create(
        &mut self,
        principal: Principal,
        return_to: ReturnTo,
        lifetime: Duration,
        now: SystemTime,
        old: Option<&Credential>,
    ) -> Result<SignedIn, SignInRefusal> {
        let secret = random_secret()?;
        self.0.retain(|_, live| live.expires > now);
        let old = match old {
            Some(Credential::Session(old)) => Some(hash(old.expose_secret())),
            _ => None,
        }
        .filter(|key| self.0.contains_key(key));
        let mut held: Vec<([u8; 32], SystemTime)> = self
            .0
            .iter()
            .filter(|(key, live)| Some(**key) != old && live.principal.id() == principal.id())
            .map(|(key, live)| (*key, live.expires))
            .collect();
        held.sort_by_key(|(_, expires)| *expires);
        let evicted: Vec<[u8; 32]> = held
            .iter()
            .take(held.len().saturating_sub(MAX_PER_PRINCIPAL - 1))
            .map(|(key, _)| *key)
            .collect();
        let removed = usize::from(old.is_some()) + evicted.len();
        if self.0.len().saturating_sub(removed) >= MAX_SESSIONS {
            return Err(SignInRefusal::TooManySessions);
        }
        for key in old.iter().chain(&evicted) {
            self.0.remove(key);
        }
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

    /// Whether a session that has not expired holds `principal`. It scans
    /// the table, which holds at most 1,024 sessions.
    pub(crate) fn holds(&self, principal: &PrincipalId, now: SystemTime) -> bool {
        self.0
            .values()
            .any(|live| live.expires > now && live.principal.id() == principal)
    }

    /// Removes a session.
    pub(crate) fn remove(&mut self, secret: &SecretString) {
        self.0.remove(&hash(secret.expose_secret()));
    }
}

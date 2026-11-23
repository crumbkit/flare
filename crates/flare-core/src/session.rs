//! Browser sessions: the rule that gives a session its life, and the
//! [`SessionStore`] trait that keeps them.
//!
//! A store keeps only the SHA-256 [`Digest`] of a session's secret, the
//! person it belongs to, when it was created and when it expires. A session
//! is sliding: it expires [`SessionRule::idle`] seconds after the person's
//! last request that extended it, and never later than
//! [`SessionRule::cap`] seconds after sign-in. A request extends the
//! session only when at least [`SessionRule::refresh`] seconds are missing
//! from its full idle life, so a busy page costs a few writes an hour and
//! not one per request.

use crate::auth::Principal;
use crate::name::PrincipalId;
use crate::time::Timestamp;
use crate::token::Digest;
use std::future::Future;

/// How long a session lasts after the last request that extended it: 12
/// hours, in seconds.
pub const SESSION_IDLE: u64 = 12 * 60 * 60;

/// How much of a session's idle life a request must have used before it
/// extends the session: 10 minutes, in seconds.
pub const SESSION_REFRESH: u64 = 10 * 60;

/// The longest a session lasts from sign-in, however often it is used: 7
/// days, in seconds.
pub const SESSION_CAP: u64 = 7 * 24 * 60 * 60;

/// How many live sessions one person holds. A sign-in at the limit ends the
/// person's session with the earliest expiry.
pub const MAX_SESSIONS_PER_PERSON: usize = 8;

/// The numbers that decide a session's life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct SessionRule {
    idle: u64,
    refresh: u64,
    cap: u64,
}

impl SessionRule {
    /// The rule with an idle life of `idle` seconds, the standard refresh
    /// of [`SESSION_REFRESH`] and the standard cap of [`SESSION_CAP`].
    #[must_use]
    pub const fn new(idle: u64) -> Self {
        Self {
            idle,
            refresh: SESSION_REFRESH,
            cap: SESSION_CAP,
        }
    }

    /// Seconds a session lasts after the request that extended it.
    #[must_use]
    pub const fn idle(&self) -> u64 {
        self.idle
    }

    /// Seconds of idle life that a request must have used to extend a
    /// session. An idle life shorter than this never extends.
    #[must_use]
    pub const fn refresh(&self) -> u64 {
        self.refresh
    }

    /// Seconds from sign-in after which a session never lives.
    #[must_use]
    pub const fn cap(&self) -> u64 {
        self.cap
    }

    /// The same rule with another cap.
    #[must_use]
    pub const fn with_cap(mut self, cap: u64) -> Self {
        self.cap = cap;
        self
    }

    /// The expiry of a session created at `created`.
    #[must_use]
    pub fn first_expiry(&self, created: Timestamp) -> Timestamp {
        self.limited(created, created)
    }

    /// The new expiry of a session created at `created` that expires at
    /// `expires` and is used at `now`, or `None` when the request leaves it
    /// as it is: fewer than [`SessionRule::refresh`] seconds of its idle
    /// life are used, or it already expires at its cap.
    #[must_use]
    pub fn extended(
        &self,
        created: Timestamp,
        expires: Timestamp,
        now: Timestamp,
    ) -> Option<Timestamp> {
        let due = now
            .as_secs()
            .saturating_add(self.idle)
            .saturating_sub(self.refresh);
        if expires.as_secs() >= due {
            return None;
        }
        let next = self.limited(created, now);
        (next > expires).then_some(next)
    }

    /// `from` plus the idle life, never past the cap after `created`.
    fn limited(&self, created: Timestamp, from: Timestamp) -> Timestamp {
        let idle = from.as_secs().saturating_add(self.idle);
        let cap = created.as_secs().saturating_add(self.cap);
        Timestamp::from_secs(idle.min(cap))
    }
}

impl Default for SessionRule {
    fn default() -> Self {
        Self::new(SESSION_IDLE)
    }
}

/// A session to create.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NewSession {
    /// The SHA-256 of the session's secret.
    pub digest: Digest,
    /// Who signed in.
    pub principal: Principal,
}

impl NewSession {
    /// A session of `principal` named by the digest of its secret.
    #[must_use]
    pub fn new(digest: Digest, principal: Principal) -> Self {
        Self { digest, principal }
    }
}

/// Why a session store could not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SessionStoreError {
    /// The store holds as many sessions as it allows, and none of them is
    /// the browser's old session or the person's earliest-expiring one.
    #[error("the session store is full")]
    Full,
    /// The store could not answer. After a write the outcome is unknown.
    #[error("the session store is unavailable")]
    Unavailable,
}

/// Keeps browser sessions by the digest of their secret.
///
/// Every read compares a session's expiry with `now` first, so a store that
/// deletes expired sessions late changes no answer.
pub trait SessionStore: Send + Sync {
    /// Creates `session` at `now`, expiring at
    /// [`SessionRule::first_expiry`]. In one step it also ends the session
    /// named by `replacing`, when that is live, and the person's session
    /// with the earliest expiry while the person holds
    /// [`MAX_SESSIONS_PER_PERSON`]. It returns the new expiry.
    ///
    /// # Errors
    ///
    /// Returns [`SessionStoreError::Full`] when a store with a limit on its
    /// whole size is at it, and [`SessionStoreError::Unavailable`] when the
    /// store cannot answer. An error leaves every live session as it was.
    fn create(
        &self,
        session: NewSession,
        replacing: Option<Digest>,
        rule: &SessionRule,
        now: Timestamp,
    ) -> impl Future<Output = Result<Timestamp, SessionStoreError>> + Send;

    /// The principal of the live session `digest`, or `None` when the
    /// session is unknown or expired. A live session whose request
    /// [`SessionRule::extended`] extends is extended in the same call, and
    /// a failure to extend it still answers the principal.
    ///
    /// # Errors
    ///
    /// Returns [`SessionStoreError::Unavailable`] when the store cannot
    /// read the session.
    fn principal(
        &self,
        digest: &Digest,
        rule: &SessionRule,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<Principal>, SessionStoreError>> + Send;

    /// When the live session `digest` expires, or `None` when it is
    /// unknown or expired. It extends nothing.
    ///
    /// # Errors
    ///
    /// Returns [`SessionStoreError::Unavailable`] when the store cannot
    /// read the session.
    fn expiry(
        &self,
        digest: &Digest,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<Timestamp>, SessionStoreError>> + Send;

    /// Whether `person` holds a live session.
    ///
    /// # Errors
    ///
    /// Returns [`SessionStoreError::Unavailable`] when the store cannot
    /// answer.
    fn holds(
        &self,
        person: &PrincipalId,
        now: Timestamp,
    ) -> impl Future<Output = Result<bool, SessionStoreError>> + Send;

    /// Ends the session `digest`. A digest that names none changes nothing.
    ///
    /// # Errors
    ///
    /// Returns [`SessionStoreError::Unavailable`] when the store cannot
    /// answer.
    fn remove(&self, digest: &Digest)
    -> impl Future<Output = Result<(), SessionStoreError>> + Send;

    /// Ends every session of `person`, which is how an operator signs one
    /// person out. Sessions are kept under the id their principal carries,
    /// which is the id the sign-in named (an allow-list entry's own id under
    /// an emails rule), so `person` is that id and not one derived from the
    /// address.
    ///
    /// # Errors
    ///
    /// Returns [`SessionStoreError::Unavailable`] when the store cannot
    /// answer.
    fn remove_all_of(
        &self,
        person: &PrincipalId,
    ) -> impl Future<Output = Result<(), SessionStoreError>> + Send;
}

#[cfg(test)]
mod tests;

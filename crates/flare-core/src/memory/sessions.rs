//! The in-memory [`SessionStore`].

use super::locked;
use crate::session::{MAX_SESSIONS_PER_PERSON, NewSession, SessionRule};
use crate::{Digest, Principal, PrincipalId, SessionStore, SessionStoreError, Timestamp};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// How many sessions the store holds at once. A sign-in at the limit,
/// after expired sessions, the browser's old session and the person's
/// earliest-expiring session beyond 8 are removed, is refused.
pub const MAX_MEMORY_SESSIONS: usize = 1024;

#[derive(Debug, Clone)]
struct Live {
    principal: Principal,
    created: Timestamp,
    expires: Timestamp,
}

/// A [`SessionStore`] that keeps its sessions in memory. Every operation is
/// one step under one lock, and a session lasts as long as the process.
#[derive(Default)]
pub struct MemorySessionStore {
    sessions: Mutex<BTreeMap<Digest, Live>>,
}

impl std::fmt::Debug for MemorySessionStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MemorySessionStore")
            .finish_non_exhaustive()
    }
}

impl MemorySessionStore {
    /// Builds an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SessionStore for MemorySessionStore {
    async fn create(
        &self,
        session: NewSession,
        replacing: Option<Digest>,
        rule: &SessionRule,
        now: Timestamp,
    ) -> Result<Timestamp, SessionStoreError> {
        let mut table = locked(&self.sessions).ok_or(SessionStoreError::Unavailable)?;
        table.retain(|_, live| live.expires > now);
        let old = replacing.filter(|digest| table.contains_key(digest));
        let mut held: Vec<(Digest, Timestamp)> = table
            .iter()
            .filter(|(digest, live)| {
                Some(**digest) != old && live.principal.id() == session.principal.id()
            })
            .map(|(digest, live)| (*digest, live.expires))
            .collect();
        held.sort_by_key(|(_, expires)| *expires);
        let evicted: Vec<Digest> = held
            .iter()
            .take(held.len().saturating_sub(MAX_SESSIONS_PER_PERSON - 1))
            .map(|(digest, _)| *digest)
            .collect();
        let removed = usize::from(old.is_some()) + evicted.len();
        if table.len().saturating_sub(removed) >= MAX_MEMORY_SESSIONS {
            return Err(SessionStoreError::Full);
        }
        for digest in old.iter().chain(&evicted) {
            table.remove(digest);
        }
        let expires = rule.first_expiry(now);
        table.insert(
            session.digest,
            Live {
                principal: session.principal,
                created: now,
                expires,
            },
        );
        Ok(expires)
    }

    async fn principal(
        &self,
        digest: &Digest,
        rule: &SessionRule,
        now: Timestamp,
    ) -> Result<Option<Principal>, SessionStoreError> {
        let mut table = locked(&self.sessions).ok_or(SessionStoreError::Unavailable)?;
        let Some(live) = table.get_mut(digest) else {
            return Ok(None);
        };
        if live.expires <= now {
            table.remove(digest);
            return Ok(None);
        }
        if let Some(next) = rule.extended(live.created, live.expires, now) {
            live.expires = next;
        }
        Ok(Some(live.principal.clone()))
    }

    async fn expiry(
        &self,
        digest: &Digest,
        now: Timestamp,
    ) -> Result<Option<Timestamp>, SessionStoreError> {
        let table = locked(&self.sessions).ok_or(SessionStoreError::Unavailable)?;
        Ok(table
            .get(digest)
            .filter(|live| live.expires > now)
            .map(|live| live.expires))
    }

    async fn holds(&self, person: &PrincipalId, now: Timestamp) -> Result<bool, SessionStoreError> {
        let table = locked(&self.sessions).ok_or(SessionStoreError::Unavailable)?;
        Ok(table
            .values()
            .any(|live| live.expires > now && live.principal.id() == person))
    }

    async fn remove(&self, digest: &Digest) -> Result<(), SessionStoreError> {
        let mut table = locked(&self.sessions).ok_or(SessionStoreError::Unavailable)?;
        table.remove(digest);
        Ok(())
    }

    async fn remove_all_of(&self, person: &PrincipalId) -> Result<(), SessionStoreError> {
        let mut table = locked(&self.sessions).ok_or(SessionStoreError::Unavailable)?;
        table.retain(|_, live| live.principal.id() != person);
        Ok(())
    }
}

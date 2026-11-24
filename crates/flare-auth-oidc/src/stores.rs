//! Object-safe forms of flare-core's [`SessionStore`] and [`SealingKeys`],
//! so one `OidcAuthenticator` type serves a memory-backed and a table-backed
//! site. The store traits return `impl Future`, which cannot be a trait
//! object, so each call here boxes its future. A sign-in or a request makes
//! a handful of calls, so the boxes cost nothing that matters.

use flare_core::{
    Digest, KeyBytes, KeyId, NewSession, Principal, PrincipalId, SealingKey, SealingKeys,
    SealingKeysError, SessionRule, SessionStore, SessionStoreError, Timestamp,
};
use std::future::Future;
use std::pin::Pin;

/// A boxed future that borrows its store.
pub(crate) type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// [`SessionStore`] as a trait object.
pub(crate) trait DynSessions: Send + Sync {
    fn create<'a>(
        &'a self,
        session: NewSession,
        replacing: Option<Digest>,
        rule: &'a SessionRule,
        now: Timestamp,
    ) -> Boxed<'a, Result<Timestamp, SessionStoreError>>;

    fn principal<'a>(
        &'a self,
        digest: &'a Digest,
        rule: &'a SessionRule,
        now: Timestamp,
    ) -> Boxed<'a, Result<Option<Principal>, SessionStoreError>>;

    fn holds<'a>(
        &'a self,
        person: &'a PrincipalId,
        now: Timestamp,
    ) -> Boxed<'a, Result<bool, SessionStoreError>>;

    fn remove<'a>(&'a self, digest: &'a Digest) -> Boxed<'a, Result<(), SessionStoreError>>;
}

impl<T: SessionStore> DynSessions for T {
    fn create<'a>(
        &'a self,
        session: NewSession,
        replacing: Option<Digest>,
        rule: &'a SessionRule,
        now: Timestamp,
    ) -> Boxed<'a, Result<Timestamp, SessionStoreError>> {
        Box::pin(SessionStore::create(self, session, replacing, rule, now))
    }

    fn principal<'a>(
        &'a self,
        digest: &'a Digest,
        rule: &'a SessionRule,
        now: Timestamp,
    ) -> Boxed<'a, Result<Option<Principal>, SessionStoreError>> {
        Box::pin(SessionStore::principal(self, digest, rule, now))
    }

    fn holds<'a>(
        &'a self,
        person: &'a PrincipalId,
        now: Timestamp,
    ) -> Boxed<'a, Result<bool, SessionStoreError>> {
        Box::pin(SessionStore::holds(self, person, now))
    }

    fn remove<'a>(&'a self, digest: &'a Digest) -> Boxed<'a, Result<(), SessionStoreError>> {
        Box::pin(SessionStore::remove(self, digest))
    }
}

/// [`SealingKeys`] as a trait object.
pub(crate) trait DynKeys: Send + Sync {
    fn current(
        &self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Boxed<'_, Result<SealingKey, SealingKeysError>>;

    fn get(
        &self,
        id: KeyId,
        now: Timestamp,
    ) -> Boxed<'_, Result<Option<SealingKey>, SealingKeysError>>;

    fn rotate(
        &self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Boxed<'_, Result<SealingKey, SealingKeysError>>;
}

impl<T: SealingKeys> DynKeys for T {
    fn current(
        &self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Boxed<'_, Result<SealingKey, SealingKeysError>> {
        Box::pin(SealingKeys::current(self, fresh, now))
    }

    fn get(
        &self,
        id: KeyId,
        now: Timestamp,
    ) -> Boxed<'_, Result<Option<SealingKey>, SealingKeysError>> {
        Box::pin(SealingKeys::get(self, id, now))
    }

    fn rotate(
        &self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Boxed<'_, Result<SealingKey, SealingKeysError>> {
        Box::pin(SealingKeys::rotate(self, fresh, now))
    }
}

//! The keys that seal pending sign-ins, and the [`SealingKeys`] trait that
//! keeps them.
//!
//! A key is 32 random bytes with a numbered id, and the id is not secret.
//! The first process with no key writes key 1. A process rotates when the
//! newest key is [`KEY_ROTATE_AFTER`] seconds old, by writing the next id
//! only if that id does not exist yet. A key stays readable until
//! [`KEY_GRACE`] seconds after the next key was written, so a sign-in that
//! began under it, which lasts 10 minutes, finishes under it.

use crate::time::Timestamp;
use secrecy::zeroize::Zeroize;
use std::fmt;
use std::future::Future;

/// How old the newest key is when a process writes the next one: 30 days,
/// in seconds.
pub const KEY_ROTATE_AFTER: u64 = 30 * 24 * 60 * 60;

/// How long a key stays readable after the next key was written: 1 hour,
/// in seconds.
pub const KEY_GRACE: u64 = 60 * 60;

/// The number of a key. Key 1 is the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyId(u32);

impl KeyId {
    /// The first key's id.
    pub const FIRST: Self = Self(1);

    /// The id `number`, or `None` for 0.
    #[must_use]
    pub const fn new(number: u32) -> Option<Self> {
        if number == 0 {
            None
        } else {
            Some(Self(number))
        }
    }

    /// The number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The next id, or `None` after the last one.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(next) => Some(Self(next)),
            None => None,
        }
    }
}

/// 32 bytes of key material. Its `Debug` output hides the bytes, and the
/// bytes are overwritten when the value is dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyBytes([u8; 32]);

impl KeyBytes {
    /// The material with these bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The bytes.
    #[must_use]
    pub const fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for KeyBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for KeyBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("KeyBytes(<hidden>)")
    }
}

/// One key: its id, its bytes and when it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SealingKey {
    /// The key's id.
    pub id: KeyId,
    /// The key's bytes.
    pub bytes: KeyBytes,
    /// When the key was written.
    pub created: Timestamp,
}

impl SealingKey {
    /// A key.
    #[must_use]
    pub fn new(id: KeyId, bytes: KeyBytes, created: Timestamp) -> Self {
        Self { id, bytes, created }
    }

    /// Whether the key is old enough that a process rotates it at `now`.
    #[must_use]
    pub fn is_due(&self, now: Timestamp) -> bool {
        now.as_secs() >= self.created.as_secs().saturating_add(KEY_ROTATE_AFTER)
    }
}

/// Why a key store could not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SealingKeysError {
    /// The store could not answer. After a write the outcome is unknown.
    #[error("the sealing key store is unavailable")]
    Unavailable,
}

/// Keeps the sealing keys.
///
/// A caller draws `fresh` bytes for each call, and the store writes them
/// only when the call creates a key. A call that loses the race to another
/// process answers the winner's key and discards `fresh`.
pub trait SealingKeys: Send + Sync {
    /// The newest key. When the store holds none, this call writes key 1
    /// from `fresh` and answers it.
    ///
    /// # Errors
    ///
    /// Returns [`SealingKeysError::Unavailable`] when the store cannot
    /// answer.
    fn current(
        &self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> impl Future<Output = Result<SealingKey, SealingKeysError>> + Send;

    /// The key `id`, or `None` when it does not exist or is no longer
    /// readable: it is not the newest and [`KEY_GRACE`] seconds have passed
    /// since the next key was written.
    ///
    /// # Errors
    ///
    /// Returns [`SealingKeysError::Unavailable`] when the store cannot
    /// answer.
    fn get(
        &self,
        id: KeyId,
        now: Timestamp,
    ) -> impl Future<Output = Result<Option<SealingKey>, SealingKeysError>> + Send;

    /// Writes the key after the newest from `fresh` when the newest is due
    /// at `now`, and only if that id does not exist yet. It answers the
    /// newest key afterwards, which is another process's when this call
    /// lost the race, and the unchanged newest key when none is due.
    ///
    /// # Errors
    ///
    /// Returns [`SealingKeysError::Unavailable`] when the store cannot
    /// answer.
    fn rotate(
        &self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> impl Future<Output = Result<SealingKey, SealingKeysError>> + Send;
}

#[cfg(test)]
mod tests;

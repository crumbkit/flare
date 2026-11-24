//! The in-memory [`SealingKeys`].

use super::locked;
use crate::sealing::KEY_GRACE;
use crate::{KeyBytes, KeyId, SealingKey, SealingKeys, SealingKeysError, Timestamp};
use std::sync::Mutex;

/// A [`SealingKeys`] that keeps its keys in memory, oldest first. Every
/// operation is one step under one lock, and the keys last as long as the
/// process.
#[derive(Default)]
pub struct MemorySealingKeys {
    keys: Mutex<Vec<SealingKey>>,
}

impl std::fmt::Debug for MemorySealingKeys {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MemorySealingKeys")
            .finish_non_exhaustive()
    }
}

impl MemorySealingKeys {
    /// Builds an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SealingKeys for MemorySealingKeys {
    async fn current(
        &self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Result<SealingKey, SealingKeysError> {
        let mut keys = locked(&self.keys).ok_or(SealingKeysError::Unavailable)?;
        if let Some(newest) = keys.last() {
            return Ok(newest.clone());
        }
        let first = SealingKey::new(KeyId::FIRST, fresh, now);
        keys.push(first.clone());
        Ok(first)
    }

    async fn get(&self, id: KeyId, now: Timestamp) -> Result<Option<SealingKey>, SealingKeysError> {
        let keys = locked(&self.keys).ok_or(SealingKeysError::Unavailable)?;
        let Some(position) = keys.iter().position(|key| key.id == id) else {
            return Ok(None);
        };
        let readable = match keys.get(position.saturating_add(1)) {
            None => true,
            Some(next) => now.as_secs() < next.created.as_secs().saturating_add(KEY_GRACE),
        };
        Ok(keys.get(position).filter(|_| readable).cloned())
    }

    async fn rotate(
        &self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Result<SealingKey, SealingKeysError> {
        let mut keys = locked(&self.keys).ok_or(SealingKeysError::Unavailable)?;
        let Some(newest) = keys.last().cloned() else {
            let first = SealingKey::new(KeyId::FIRST, fresh, now);
            keys.push(first.clone());
            return Ok(first);
        };
        if !newest.is_due(now) {
            return Ok(newest);
        }
        let Some(id) = newest.id.next() else {
            return Ok(newest);
        };
        let next = SealingKey::new(id, fresh, now);
        keys.push(next.clone());
        Ok(next)
    }
}

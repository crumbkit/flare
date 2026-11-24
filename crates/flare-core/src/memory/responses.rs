//! The in-memory [`ResponseStore`].

use super::{MemorySequence, locked};
use crate::store::{ResponseStore, ResponseStoreError};
use crate::{IndexEntry, NewEntry, NewLink, Position, Reader, ResponseLink, ShareId};
use std::sync::Mutex;

/// The links and the index entries, under one lock. Their positions come
/// from the store's [`MemorySequence`], taken while the lock is held.
#[derive(Default)]
struct State {
    /// The links, in position order.
    links: Vec<ResponseLink>,
    /// The index entries, in position order.
    entries: Vec<IndexEntry>,
}

/// A [`ResponseStore`] that keeps its links and index entries in memory,
/// in position order. Every operation is one step under one lock.
#[derive(Default)]
pub struct MemoryResponseStore {
    state: Mutex<State>,
    sequence: MemorySequence,
}

impl std::fmt::Debug for MemoryResponseStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MemoryResponseStore")
            .finish_non_exhaustive()
    }
}

impl MemoryResponseStore {
    /// Builds an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A store that takes its positions from `sequence`, which a
    /// [`MemoryCommentStore`](super::MemoryCommentStore) built with the
    /// same sequence shares.
    #[must_use]
    pub fn with_sequence(sequence: MemorySequence) -> Self {
        Self {
            state: Mutex::default(),
            sequence,
        }
    }

    /// The next position from the shared sequence, which becomes the head.
    /// Callers hold the state lock, so a reader that reads the head and
    /// then the state sees every link and entry at or below the head.
    fn next(&self) -> Result<Position, ResponseStoreError> {
        self.sequence.next().ok_or(ResponseStoreError::Unavailable)
    }

    /// Every activity index entry, in position order, for tests that check
    /// the whole index. A poisoned lock answers no entries.
    #[must_use]
    pub fn entries(&self) -> Vec<IndexEntry> {
        locked(&self.state).map_or_else(Vec::new, |state| state.entries.clone())
    }
}

impl ResponseStore for MemoryResponseStore {
    async fn link(&self, link: NewLink) -> Result<ResponseLink, ResponseStoreError> {
        let mut state = locked(&self.state).ok_or(ResponseStoreError::Unavailable)?;
        if let Some(first) = state
            .links
            .iter()
            .find(|old| old.response == link.response && old.flare == link.flare)
        {
            return Ok(first.clone());
        }
        let stored = link.at_position(self.next()?);
        state.links.push(stored.clone());
        Ok(stored)
    }

    async fn head(&self) -> Result<Position, ResponseStoreError> {
        self.sequence.head().ok_or(ResponseStoreError::Unavailable)
    }

    async fn to_flare(
        &self,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> Result<Vec<ResponseLink>, ResponseStoreError> {
        let state = locked(&self.state).ok_or(ResponseStoreError::Unavailable)?;
        Ok(state
            .links
            .iter()
            .filter(|link| &link.flare == flare && link.position > after && link.position <= upto)
            .take(limit)
            .cloned()
            .collect())
    }

    async fn of_response(
        &self,
        response: &ShareId,
    ) -> Result<Vec<ResponseLink>, ResponseStoreError> {
        let state = locked(&self.state).ok_or(ResponseStoreError::Unavailable)?;
        Ok(state
            .links
            .iter()
            .filter(|link| &link.response == response)
            .cloned()
            .collect())
    }

    async fn pairs(
        &self,
        responses: &[ShareId],
        flare: &ShareId,
    ) -> Result<Vec<ResponseLink>, ResponseStoreError> {
        let state = locked(&self.state).ok_or(ResponseStoreError::Unavailable)?;
        Ok(responses
            .iter()
            .filter_map(|response| {
                state
                    .links
                    .iter()
                    .find(|link| &link.response == response && &link.flare == flare)
                    .cloned()
            })
            .collect())
    }

    async fn index(&self, entries: Vec<NewEntry>) -> Result<(), ResponseStoreError> {
        let mut state = locked(&self.state).ok_or(ResponseStoreError::Unavailable)?;
        for entry in entries {
            let held = state.entries.iter().any(|old| {
                old.reader == entry.reader
                    && old.response == entry.link.response
                    && old.flare == entry.link.flare
            });
            if !held {
                let position = self.next()?;
                state.entries.push(entry.at_position(position));
            }
        }
        Ok(())
    }

    async fn unindex(&self, reader: &Reader, response: &ShareId) -> Result<(), ResponseStoreError> {
        let mut state = locked(&self.state).ok_or(ResponseStoreError::Unavailable)?;
        state
            .entries
            .retain(|entry| !(&entry.reader == reader && &entry.response == response));
        Ok(())
    }

    async fn visible(
        &self,
        reader: &Reader,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> Result<Vec<IndexEntry>, ResponseStoreError> {
        let state = locked(&self.state).ok_or(ResponseStoreError::Unavailable)?;
        Ok(state
            .entries
            .iter()
            .filter(|entry| {
                &entry.reader == reader
                    && &entry.flare == flare
                    && entry.position > after
                    && entry.position <= upto
            })
            .take(limit)
            .cloned()
            .collect())
    }
}

//! The in-memory [`ResponseStore`].

use super::locked;
use crate::store::{ResponseStore, ResponseStoreError};
use crate::{IndexEntry, NewEntry, NewLink, Position, Reader, ResponseLink, ShareId};
use std::sync::Mutex;

/// The links, the index entries and the head, all under one lock.
#[derive(Default)]
struct State {
    /// The last position given to a link or an entry.
    head: u64,
    /// The links, in position order.
    links: Vec<ResponseLink>,
    /// The index entries, in position order.
    entries: Vec<IndexEntry>,
}

impl State {
    /// The next position, which becomes the head.
    fn next(&mut self) -> Result<Position, ResponseStoreError> {
        self.head = self
            .head
            .checked_add(1)
            .ok_or(ResponseStoreError::Unavailable)?;
        Ok(Position::new(self.head))
    }
}

/// A [`ResponseStore`] that keeps its links and index entries in memory,
/// in position order. Every operation is one step under one lock.
#[derive(Default)]
pub struct MemoryResponseStore {
    state: Mutex<State>,
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
        let stored = link.at_position(state.next()?);
        state.links.push(stored.clone());
        Ok(stored)
    }

    async fn head(&self) -> Result<Position, ResponseStoreError> {
        let state = locked(&self.state).ok_or(ResponseStoreError::Unavailable)?;
        Ok(Position::new(state.head))
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
                let position = state.next()?;
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

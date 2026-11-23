//! The in-memory [`ResponseStore`].

use super::{MemorySequence, locked};
use crate::store::{ResponseStore, ResponseStoreError};
use crate::{NewLink, Position, ResponseLink, ShareId};
use std::sync::Mutex;

/// A [`ResponseStore`] that keeps its links in memory, in position order.
/// Every operation is one step under one lock.
#[derive(Default)]
pub struct MemoryResponseStore {
    links: Mutex<Vec<ResponseLink>>,
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
            links: Mutex::default(),
            sequence,
        }
    }
}

impl ResponseStore for MemoryResponseStore {
    async fn link(&self, link: NewLink) -> Result<ResponseLink, ResponseStoreError> {
        let mut links = locked(&self.links).ok_or(ResponseStoreError::Unavailable)?;
        if let Some(first) = links
            .iter()
            .find(|old| old.response == link.response && old.flare == link.flare)
        {
            return Ok(first.clone());
        }
        let position = self
            .sequence
            .next()
            .ok_or(ResponseStoreError::Unavailable)?;
        let stored = link.at_position(position);
        links.push(stored.clone());
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
        let links = locked(&self.links).ok_or(ResponseStoreError::Unavailable)?;
        Ok(links
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
        let links = locked(&self.links).ok_or(ResponseStoreError::Unavailable)?;
        Ok(links
            .iter()
            .filter(|link| &link.response == response)
            .cloned()
            .collect())
    }
}

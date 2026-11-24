//! The position counter that the in-memory response and comment stores
//! share, as the DynamoDB stores share one counter item.

use super::locked;
use crate::response::Position;
use std::sync::{Arc, Mutex};

/// The one increasing sequence of positions. Clones share it.
#[derive(Debug, Clone, Default)]
pub struct MemorySequence {
    head: Arc<Mutex<u64>>,
}

impl MemorySequence {
    /// A new sequence with head 0.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The highest position given, or `None` after a poisoned lock.
    pub(super) fn head(&self) -> Option<Position> {
        locked(&self.head).map(|head| Position::new(*head))
    }

    /// Gives the next position, or `None` after a poisoned lock.
    pub(super) fn next(&self) -> Option<Position> {
        let mut head = locked(&self.head)?;
        *head = head.saturating_add(1);
        Some(Position::new(*head))
    }
}

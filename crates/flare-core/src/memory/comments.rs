//! The in-memory [`CommentStore`] and [`CursorStore`].

use super::{MemorySequence, locked};
use crate::comment::{
    Comment, CommentAction, CommentBody, CommentEvent, CommentId, CommentKey, CursorName,
    MAX_COMMENTS_PER_FLARE, MAX_CURSORS_PER_PERSON, NewComment,
};
use crate::name::PrincipalId;
use crate::response::Position;
use crate::share::ShareId;
use crate::store::{Added, CommentStore, CommentStoreError, CursorStore};
use crate::time::Timestamp;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
struct State {
    comments: Vec<Comment>,
    events: Vec<CommentEvent>,
    keys: HashMap<(PrincipalId, ShareId, CommentKey), CommentId>,
}

/// A [`CommentStore`] that keeps its comments in memory. Every operation is
/// one step under one lock, and a position is taken in that step.
pub struct MemoryCommentStore {
    state: Mutex<State>,
    sequence: MemorySequence,
    limit: usize,
}

impl Default for MemoryCommentStore {
    fn default() -> Self {
        Self::with_sequence(MemorySequence::new())
    }
}

impl std::fmt::Debug for MemoryCommentStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MemoryCommentStore")
            .finish_non_exhaustive()
    }
}

impl MemoryCommentStore {
    /// A store with a sequence of its own.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A store that takes its positions from `sequence`, which a
    /// [`MemoryResponseStore`](super::MemoryResponseStore) built with the
    /// same sequence shares.
    #[must_use]
    pub fn with_sequence(sequence: MemorySequence) -> Self {
        Self {
            state: Mutex::default(),
            sequence,
            limit: MAX_COMMENTS_PER_FLARE,
        }
    }

    /// The same store with `limit` comments per flare, so a test reaches
    /// the limit with a few comments.
    #[must_use]
    pub fn with_flare_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    fn position(&self) -> Result<Position, CommentStoreError> {
        self.sequence.next().ok_or(CommentStoreError::Unavailable)
    }
}

impl CommentStore for MemoryCommentStore {
    async fn add(&self, new: NewComment) -> Result<Added, CommentStoreError> {
        let mut state = locked(&self.state).ok_or(CommentStoreError::Unavailable)?;
        let slot = (new.author.clone(), new.flare.clone(), new.key.clone());
        if let Some(first) = state.keys.get(&slot)
            && let Some(found) = state.comments.iter().find(|held| &held.id == first)
        {
            return Ok(Added::Repeated(found.clone()));
        }
        let held = state
            .comments
            .iter()
            .filter(|held| held.flare == new.flare)
            .count();
        if held >= self.limit {
            return Err(CommentStoreError::TooMany);
        }
        let position = self.position()?;
        let at = new.at;
        let comment = Comment::from_new(new, position);
        state.keys.insert(slot, comment.id.clone());
        state.events.push(CommentEvent::of(
            &comment,
            CommentAction::Created,
            position,
            at,
        ));
        state.comments.push(comment.clone());
        Ok(Added::Created(comment))
    }

    async fn get(&self, id: &CommentId) -> Result<Comment, CommentStoreError> {
        let state = locked(&self.state).ok_or(CommentStoreError::Unavailable)?;
        state
            .comments
            .iter()
            .find(|held| &held.id == id)
            .cloned()
            .ok_or(CommentStoreError::NotFound)
    }

    async fn of_flare(&self, flare: &ShareId) -> Result<Vec<Comment>, CommentStoreError> {
        let state = locked(&self.state).ok_or(CommentStoreError::Unavailable)?;
        Ok(state
            .comments
            .iter()
            .filter(|held| &held.flare == flare)
            .cloned()
            .collect())
    }

    async fn edit(
        &self,
        id: &CommentId,
        body: CommentBody,
        at: Timestamp,
    ) -> Result<Comment, CommentStoreError> {
        let mut state = locked(&self.state).ok_or(CommentStoreError::Unavailable)?;
        let state = &mut *state;
        let held = state
            .comments
            .iter_mut()
            .find(|held| &held.id == id)
            .ok_or(CommentStoreError::NotFound)?;
        if held.is_deleted() {
            return Err(CommentStoreError::Deleted);
        }
        let position = self.position()?;
        held.body = Some(body);
        held.edited = Some(at);
        state
            .events
            .push(CommentEvent::of(held, CommentAction::Edited, position, at));
        Ok(held.clone())
    }

    async fn delete(&self, id: &CommentId, at: Timestamp) -> Result<Comment, CommentStoreError> {
        let mut state = locked(&self.state).ok_or(CommentStoreError::Unavailable)?;
        let state = &mut *state;
        let held = state
            .comments
            .iter_mut()
            .find(|held| &held.id == id)
            .ok_or(CommentStoreError::NotFound)?;
        if held.is_deleted() {
            return Ok(held.clone());
        }
        let position = self.position()?;
        held.body = None;
        held.anchor = None;
        state
            .events
            .push(CommentEvent::of(held, CommentAction::Deleted, position, at));
        Ok(held.clone())
    }

    async fn events(
        &self,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> Result<Vec<CommentEvent>, CommentStoreError> {
        let state = locked(&self.state).ok_or(CommentStoreError::Unavailable)?;
        Ok(state
            .events
            .iter()
            .filter(|held| &held.flare == flare && held.position > after && held.position <= upto)
            .take(limit)
            .cloned()
            .collect())
    }
}

/// A [`CursorStore`] that keeps its cursors in memory.
pub struct MemoryCursorStore {
    cursors: Mutex<HashMap<(PrincipalId, CursorName), Position>>,
    limit: usize,
}

impl Default for MemoryCursorStore {
    fn default() -> Self {
        Self {
            cursors: Mutex::default(),
            limit: MAX_CURSORS_PER_PERSON,
        }
    }
}

impl std::fmt::Debug for MemoryCursorStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MemoryCursorStore")
            .finish_non_exhaustive()
    }
}

impl MemoryCursorStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The same store with `limit` cursors per person, so a test reaches
    /// the limit with a few cursors.
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

impl CursorStore for MemoryCursorStore {
    async fn get(
        &self,
        owner: &PrincipalId,
        name: &CursorName,
    ) -> Result<Option<Position>, CommentStoreError> {
        let cursors = locked(&self.cursors).ok_or(CommentStoreError::Unavailable)?;
        Ok(cursors.get(&(owner.clone(), name.clone())).copied())
    }

    async fn advance(
        &self,
        owner: &PrincipalId,
        name: &CursorName,
        to: Position,
    ) -> Result<Position, CommentStoreError> {
        let mut cursors = locked(&self.cursors).ok_or(CommentStoreError::Unavailable)?;
        let key = (owner.clone(), name.clone());
        if let Some(held) = cursors.get_mut(&key) {
            *held = (*held).max(to);
            return Ok(*held);
        }
        let own = cursors.keys().filter(|(who, _)| who == owner).count();
        if own >= self.limit {
            return Err(CommentStoreError::TooMany);
        }
        cursors.insert(key, to);
        Ok(to)
    }
}

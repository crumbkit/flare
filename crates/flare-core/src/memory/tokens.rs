//! The in-memory [`TokenStore`].

use super::locked;
use crate::store::{TokenStore, TokenStoreError};
use crate::{Digest, Email, LoginCode, NewToken, StoredToken, Timestamp, TokenId, UsedCode};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// A login code and whether it was used.
struct Code {
    code: LoginCode,
    used: bool,
}

#[derive(Default)]
struct State {
    codes: BTreeMap<Digest, Code>,
    tokens: BTreeMap<TokenId, StoredToken>,
}

/// A [`TokenStore`] that keeps its codes and tokens in memory. Every
/// operation is one step under one lock.
#[derive(Default)]
pub struct MemoryTokenStore {
    state: Mutex<State>,
}

impl std::fmt::Debug for MemoryTokenStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MemoryTokenStore")
            .finish_non_exhaustive()
    }
}

impl MemoryTokenStore {
    /// Builds an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStore for MemoryTokenStore {
    async fn issue_code(&self, code: LoginCode) -> Result<(), TokenStoreError> {
        let mut state = locked(&self.state).ok_or(TokenStoreError::Unavailable)?;
        if state.codes.contains_key(code.digest()) {
            return Err(TokenStoreError::Taken);
        }
        state
            .codes
            .insert(*code.digest(), Code { code, used: false });
        Ok(())
    }

    async fn use_code(&self, digest: &Digest, now: Timestamp) -> Result<UsedCode, TokenStoreError> {
        let mut state = locked(&self.state).ok_or(TokenStoreError::Unavailable)?;
        let entry = state
            .codes
            .get_mut(digest)
            .ok_or(TokenStoreError::CodeRefused)?;
        if entry.used || !entry.code.is_live(now) {
            return Err(TokenStoreError::CodeRefused);
        }
        entry.used = true;
        Ok(entry.code.used())
    }

    async fn create(&self, token: NewToken) -> Result<Option<TokenId>, TokenStoreError> {
        let mut state = locked(&self.state).ok_or(TokenStoreError::Unavailable)?;
        if state.tokens.contains_key(token.id()) {
            return Err(TokenStoreError::Taken);
        }
        let created = token.created();
        let previous = state
            .tokens
            .values()
            .find(|old| {
                old.owner() == token.owner()
                    && old.label() == token.label()
                    && old.revoked().is_none()
            })
            .map(|old| old.id().clone());
        if let Some(id) = &previous
            && let Some(old) = state.tokens.remove(id)
        {
            state.tokens.insert(id.clone(), old.revoked_at(created));
        }
        state.tokens.insert(token.id().clone(), token.stored());
        Ok(previous)
    }

    async fn get(&self, id: &TokenId) -> Result<StoredToken, TokenStoreError> {
        let state = locked(&self.state).ok_or(TokenStoreError::Unavailable)?;
        state
            .tokens
            .get(id)
            .cloned()
            .ok_or(TokenStoreError::NotFound)
    }

    async fn revoke(
        &self,
        id: &TokenId,
        owner: &Email,
        at: Timestamp,
    ) -> Result<(), TokenStoreError> {
        let mut state = locked(&self.state).ok_or(TokenStoreError::Unavailable)?;
        let token = state.tokens.remove(id).ok_or(TokenStoreError::NotFound)?;
        if token.owner() != owner {
            state.tokens.insert(id.clone(), token);
            return Err(TokenStoreError::NotFound);
        }
        state.tokens.insert(id.clone(), token.revoked_at(at));
        Ok(())
    }
}

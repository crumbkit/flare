//! The in-memory [`ShareStore`].

use super::locked;
use crate::share::{MAX_SHARES_PER_OWNER, MAX_VERSIONS};
use crate::store::{ShareStore, ShareStoreError};
use crate::{
    Listing, NewShare, NewVersion, PrincipalId, Share, ShareId, ShareVersion, SharedVersion,
};
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// One share and its versions, oldest first, so version `n` sits at index
/// `n - 1`.
struct Entry {
    share: Share,
    versions: Vec<SharedVersion>,
}

/// A [`ShareStore`] that keeps its shares in memory. Every operation is one
/// step under one lock.
#[derive(Default)]
pub struct MemoryShareStore {
    state: Mutex<BTreeMap<ShareId, Entry>>,
}

impl std::fmt::Debug for MemoryShareStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MemoryShareStore")
            .finish_non_exhaustive()
    }
}

impl MemoryShareStore {
    /// Builds an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl ShareStore for MemoryShareStore {
    async fn create(&self, new: NewShare) -> Result<Share, ShareStoreError> {
        let mut shares = locked(&self.state).ok_or(ShareStoreError::Unavailable)?;
        if shares.contains_key(new.id()) {
            return Err(ShareStoreError::IdTaken);
        }
        let owned = shares
            .values()
            .filter(|entry| entry.share.owner() == new.owner())
            .count();
        if owned >= MAX_SHARES_PER_OWNER {
            return Err(ShareStoreError::TooManyShares);
        }
        let share = new.share();
        let entry = Entry {
            share: share.clone(),
            versions: vec![new.first_version()],
        };
        shares.insert(new.id().clone(), entry);
        Ok(share)
    }

    async fn get(&self, id: &ShareId) -> Result<Share, ShareStoreError> {
        let shares = locked(&self.state).ok_or(ShareStoreError::Unavailable)?;
        shares
            .get(id)
            .map(|entry| entry.share.clone())
            .ok_or(ShareStoreError::NotFound)
    }

    async fn publish(
        &self,
        id: &ShareId,
        version: NewVersion,
    ) -> Result<ShareVersion, ShareStoreError> {
        let mut shares = locked(&self.state).ok_or(ShareStoreError::Unavailable)?;
        let entry = shares.get_mut(id).ok_or(ShareStoreError::NotFound)?;
        let latest = entry.share.latest();
        if latest.get() >= MAX_VERSIONS {
            return Err(ShareStoreError::TooManyVersions);
        }
        let next = latest.next().ok_or(ShareStoreError::TooManyVersions)?;
        entry.versions.push(version.shared(id.clone(), next));
        entry.share = entry.share.clone().with_latest(next);
        Ok(next)
    }

    async fn version(
        &self,
        id: &ShareId,
        number: ShareVersion,
    ) -> Result<SharedVersion, ShareStoreError> {
        let shares = locked(&self.state).ok_or(ShareStoreError::Unavailable)?;
        let entry = shares.get(id).ok_or(ShareStoreError::NotFound)?;
        let index = usize::try_from(number.get().saturating_sub(1))
            .map_err(|_| ShareStoreError::NotFound)?;
        entry
            .versions
            .get(index)
            .cloned()
            .ok_or(ShareStoreError::NotFound)
    }

    async fn owned_by(&self, owner: &PrincipalId) -> Result<Listing<Share>, ShareStoreError> {
        let shares = locked(&self.state).ok_or(ShareStoreError::Unavailable)?;
        let mut owned: Vec<Share> = shares
            .values()
            .filter(|entry| entry.share.owner() == owner)
            .map(|entry| entry.share.clone())
            .collect();
        owned.sort_by_key(|share| (Reverse(share.created()), share.id().clone()));
        Ok(Listing::new(owned))
    }
}

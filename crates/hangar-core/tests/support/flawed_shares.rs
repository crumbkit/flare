//! Deliberately wrong share and file stores, for the tests that show the
//! share and file store suites catch the flaw each clause exists to catch.
//! Each flaw breaks one clause and keeps every other rule.

use hangar_core::{
    FileId, FileStore, FileStoreError, Listing, MAX_SHARES_PER_OWNER, MAX_VERSIONS, NewFile,
    NewShare, NewVersion, PrincipalId, Share, ShareId, ShareStore, ShareStoreError, ShareVersion,
    SharedVersion, StoredFile,
};
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::sync::{Barrier, Mutex};

/// The one rule a [`FlawedShares`] breaks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShareFlaw {
    /// `publish` reads the latest number, waits for the other caller, and
    /// then writes the number after the one it read, so two publishes that
    /// run together reuse one number. Only the concurrency clause may run
    /// against it, because a lone publish waits for ever.
    ReusesANumberInARace,
    /// `create` with a taken id replaces the first share.
    OverwritesATakenId,
    /// `publish` of version 2 also replaces version 1's content.
    ChangesTheFirstVersionOnPublish,
    /// `owned_by` lists every owner's shares.
    ListsOtherOwners,
    /// `owned_by` answers the list as the previous call saw it.
    ListsLate,
    /// `publish` stores the version without moving `latest`.
    KeepsLatest,
    /// `create` writes the share without its version 1.
    WritesNoFirstVersion,
    /// `version` above `latest` answers the latest version.
    ServesAboveLatest,
    /// `create` has no limit per owner.
    NoShareLimit,
    /// `publish` has no limit per share.
    NoVersionLimit,
}

#[derive(Default)]
struct ShareState {
    shares: BTreeMap<ShareId, (Share, BTreeMap<u32, SharedVersion>)>,
    listed: BTreeMap<PrincipalId, Vec<Share>>,
}

/// A share store with one [`ShareFlaw`].
pub(crate) struct FlawedShares {
    flaw: ShareFlaw,
    state: Mutex<ShareState>,
    barrier: Barrier,
}

impl FlawedShares {
    pub(crate) fn new(flaw: ShareFlaw) -> Self {
        Self {
            flaw,
            state: Mutex::new(ShareState::default()),
            barrier: Barrier::new(2),
        }
    }

    fn owned(state: &ShareState, owner: &PrincipalId, everyone: bool) -> Vec<Share> {
        let mut owned: Vec<Share> = state
            .shares
            .values()
            .map(|(share, _)| share.clone())
            .filter(|share| everyone || share.owner() == owner)
            .collect();
        owned.sort_by_key(|share| (Reverse(share.created()), share.id().clone()));
        owned
    }
}

impl ShareStore for FlawedShares {
    async fn create(&self, new: NewShare) -> Result<Share, ShareStoreError> {
        let mut state = self.state.lock().unwrap();
        if state.shares.contains_key(new.id()) && self.flaw != ShareFlaw::OverwritesATakenId {
            return Err(ShareStoreError::IdTaken);
        }
        let owned = Self::owned(&state, new.owner(), false).len();
        if owned >= MAX_SHARES_PER_OWNER && self.flaw != ShareFlaw::NoShareLimit {
            return Err(ShareStoreError::TooManyShares);
        }
        let mut versions = BTreeMap::new();
        if self.flaw != ShareFlaw::WritesNoFirstVersion {
            versions.insert(1, new.first_version());
        }
        state
            .shares
            .insert(new.id().clone(), (new.share(), versions));
        Ok(new.share())
    }

    async fn get(&self, id: &ShareId) -> Result<Share, ShareStoreError> {
        let state = self.state.lock().unwrap();
        state
            .shares
            .get(id)
            .map(|(share, _)| share.clone())
            .ok_or(ShareStoreError::NotFound)
    }

    async fn publish(
        &self,
        id: &ShareId,
        version: NewVersion,
    ) -> Result<ShareVersion, ShareStoreError> {
        let mut state = self.state.lock().unwrap();
        let latest = state
            .shares
            .get(id)
            .map(|(share, _)| share.latest())
            .ok_or(ShareStoreError::NotFound)?;
        if latest.get() >= MAX_VERSIONS && self.flaw != ShareFlaw::NoVersionLimit {
            return Err(ShareStoreError::TooManyVersions);
        }
        if self.flaw == ShareFlaw::ReusesANumberInARace {
            drop(state);
            self.barrier.wait();
            state = self.state.lock().unwrap();
        }
        let next = latest.next().unwrap();
        let (share, versions) = state.shares.get_mut(id).unwrap();
        versions.insert(next.get(), version.shared(id.clone(), next));
        if self.flaw == ShareFlaw::ChangesTheFirstVersionOnPublish && next.get() == 2 {
            versions.insert(1, version.shared(id.clone(), ShareVersion::first()));
        }
        if self.flaw != ShareFlaw::KeepsLatest {
            *share = share.clone().with_latest(next);
        }
        Ok(next)
    }

    async fn version(
        &self,
        id: &ShareId,
        number: ShareVersion,
    ) -> Result<SharedVersion, ShareStoreError> {
        let state = self.state.lock().unwrap();
        let (share, versions) = state.shares.get(id).ok_or(ShareStoreError::NotFound)?;
        if number > share.latest() {
            if self.flaw == ShareFlaw::ServesAboveLatest {
                return versions
                    .get(&share.latest().get())
                    .cloned()
                    .ok_or(ShareStoreError::NotFound);
            }
            return Err(ShareStoreError::NotFound);
        }
        versions
            .get(&number.get())
            .cloned()
            .ok_or(ShareStoreError::NotFound)
    }

    async fn owned_by(&self, owner: &PrincipalId) -> Result<Listing<Share>, ShareStoreError> {
        let mut state = self.state.lock().unwrap();
        let everyone = self.flaw == ShareFlaw::ListsOtherOwners;
        let now = Self::owned(&state, owner, everyone);
        if self.flaw == ShareFlaw::ListsLate {
            let before = state.listed.insert(owner.clone(), now).unwrap_or_default();
            return Ok(Listing::new(before));
        }
        Ok(Listing::new(now))
    }
}

/// The one rule a [`FlawedFiles`] breaks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileFlaw {
    /// Files are keyed by id alone, so a file reaches across shares.
    IgnoresTheShare,
    /// `put` with a taken id replaces the first file.
    OverwritesATakenId,
}

/// A file store with one [`FileFlaw`].
pub(crate) struct FlawedFiles {
    flaw: FileFlaw,
    files: Mutex<BTreeMap<(Option<ShareId>, FileId), NewFile>>,
}

impl FlawedFiles {
    pub(crate) fn new(flaw: FileFlaw) -> Self {
        Self {
            flaw,
            files: Mutex::new(BTreeMap::new()),
        }
    }

    fn key(&self, share: &ShareId, id: &FileId) -> (Option<ShareId>, FileId) {
        let share = (self.flaw != FileFlaw::IgnoresTheShare).then(|| share.clone());
        (share, id.clone())
    }
}

impl FileStore for FlawedFiles {
    async fn put(&self, file: NewFile) -> Result<(), FileStoreError> {
        let key = self.key(file.share(), file.id());
        let mut files = self.files.lock().unwrap();
        if files.contains_key(&key) && self.flaw != FileFlaw::OverwritesATakenId {
            return Err(FileStoreError::IdTaken);
        }
        files.insert(key, file);
        Ok(())
    }

    async fn get(&self, share: &ShareId, id: &FileId) -> Result<StoredFile, FileStoreError> {
        let files = self.files.lock().unwrap();
        files
            .get(&self.key(share, id))
            .cloned()
            .map(StoredFile::from)
            .ok_or(FileStoreError::NotFound)
    }
}

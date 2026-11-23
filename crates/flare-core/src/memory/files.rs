//! The in-memory [`FileStore`].

use super::locked;
use crate::store::{FileStore, FileStoreError};
use crate::{FileId, NewFile, ShareId, StoredFile};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::sync::Mutex;

/// A [`FileStore`] that keeps its files in memory, keyed by share and id.
#[derive(Default)]
pub struct MemoryFileStore {
    state: Mutex<BTreeMap<(ShareId, FileId), StoredFile>>,
}

impl std::fmt::Debug for MemoryFileStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MemoryFileStore")
            .finish_non_exhaustive()
    }
}

impl MemoryFileStore {
    /// Builds an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl FileStore for MemoryFileStore {
    async fn put(&self, file: NewFile) -> Result<(), FileStoreError> {
        let mut files = locked(&self.state).ok_or(FileStoreError::Unavailable)?;
        match files.entry((file.share().clone(), file.id().clone())) {
            Entry::Occupied(_) => Err(FileStoreError::IdTaken),
            Entry::Vacant(slot) => {
                slot.insert(StoredFile::from(file));
                Ok(())
            }
        }
    }

    async fn get(&self, share: &ShareId, id: &FileId) -> Result<StoredFile, FileStoreError> {
        let files = locked(&self.state).ok_or(FileStoreError::Unavailable)?;
        files
            .get(&(share.clone(), id.clone()))
            .cloned()
            .ok_or(FileStoreError::NotFound)
    }
}

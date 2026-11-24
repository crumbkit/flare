//! The in-memory [`FileStore`].

use super::locked;
use crate::store::{FileCursor, FilePage, FileStore, FileStoreError, ListedFile, MAX_FILE_PAGE};
use crate::{FileId, NewFile, ShareId, StoredFile, Timestamp};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::ops::Bound;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// The key of one file: its share and its id.
type Key = (ShareId, FileId);

/// A [`FileStore`] that keeps its files in memory, keyed by share and id,
/// each with the time it was stored. The time is the system clock's unless
/// a test sets it with [`MemoryFileStore::set_time`]. A list walks the
/// files in key order, and its cursor is the last key it named.
#[derive(Default)]
pub struct MemoryFileStore {
    state: Mutex<BTreeMap<Key, (StoredFile, Timestamp)>>,
    time: Mutex<Option<Timestamp>>,
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

    /// Stamps every later put with `time` in place of the system clock.
    pub fn set_time(&self, time: Timestamp) {
        if let Some(mut slot) = locked(&self.time) {
            *slot = Some(time);
        }
    }

    /// The time a put now stores.
    fn now(&self) -> Timestamp {
        let set = locked(&self.time).and_then(|slot| *slot);
        set.unwrap_or_else(|| {
            Timestamp::from_secs(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |since| since.as_secs()),
            )
        })
    }
}

/// The text of a cursor after `key`.
fn cursor_of((share, id): &Key) -> FileCursor {
    FileCursor::new(format!("{}/{}", share.as_str(), id.as_str()))
}

/// The key a cursor names, or `None` for a cursor this store did not write.
fn key_of(cursor: &FileCursor) -> Option<Key> {
    let (share, id) = cursor.as_str().split_once('/')?;
    Some((ShareId::try_from(share).ok()?, FileId::try_from(id).ok()?))
}

impl FileStore for MemoryFileStore {
    async fn put(&self, file: NewFile) -> Result<(), FileStoreError> {
        let now = self.now();
        let mut files = locked(&self.state).ok_or(FileStoreError::Unavailable)?;
        match files.entry((file.share().clone(), file.id().clone())) {
            Entry::Occupied(_) => Err(FileStoreError::IdTaken),
            Entry::Vacant(slot) => {
                slot.insert((StoredFile::from(file), now));
                Ok(())
            }
        }
    }

    async fn get(&self, share: &ShareId, id: &FileId) -> Result<StoredFile, FileStoreError> {
        let files = locked(&self.state).ok_or(FileStoreError::Unavailable)?;
        files
            .get(&(share.clone(), id.clone()))
            .map(|(file, _)| file.clone())
            .ok_or(FileStoreError::NotFound)
    }

    async fn delete(&self, share: &ShareId, id: &FileId) -> Result<(), FileStoreError> {
        let mut files = locked(&self.state).ok_or(FileStoreError::Unavailable)?;
        files.remove(&(share.clone(), id.clone()));
        Ok(())
    }

    async fn list(
        &self,
        after: Option<&FileCursor>,
        limit: usize,
    ) -> Result<FilePage, FileStoreError> {
        let limit = limit.clamp(1, MAX_FILE_PAGE);
        let start = match after {
            Some(cursor) => Bound::Excluded(key_of(cursor).ok_or(FileStoreError::Unavailable)?),
            None => Bound::Unbounded,
        };
        let files = locked(&self.state).ok_or(FileStoreError::Unavailable)?;
        let mut walk = files.range((start, Bound::Unbounded));
        let page: Vec<(&Key, Timestamp)> = walk
            .by_ref()
            .take(limit)
            .map(|(key, (_, stored))| (key, *stored))
            .collect();
        let more = walk.next().is_some();
        let next = page.last().filter(|_| more).map(|(key, _)| cursor_of(key));
        let listed = page
            .into_iter()
            .map(|((share, id), stored)| ListedFile::new(share.clone(), id.clone(), stored))
            .collect();
        Ok(FilePage::new(listed, next))
    }
}

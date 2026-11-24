//! Stores that fail on cue, for the upload's crash points.

use crate::clock::Clock;
use flare_core::memory::{MemoryFileStore, MemoryShareStore};
use flare_core::{
    Creators, CreatorsError, FileCursor, FileId, FilePage, FileStore, FileStoreError, Listing,
    NewFile, NewShare, NewVersion, Principal, PrincipalId, Share, ShareId, ShareStore,
    ShareStoreError, ShareVersion, SharedVersion, StoredFile,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// What the next create or publish does in place of passing.
#[derive(Debug, Clone, Copy)]
pub(super) enum Commit {
    /// Writes nothing and answers the error.
    Refuse(ShareStoreError),
    /// Writes, and answers `Unavailable` as a lost reply would.
    WriteThenLose,
}

/// A share store over memory whose next creates and publishes follow a
/// script, and pass once it runs out.
#[derive(Clone, Default)]
pub(super) struct Shares(Arc<SharesInner>);

#[derive(Default)]
pub(super) struct SharesInner {
    store: MemoryShareStore,
    creates: Mutex<VecDeque<Commit>>,
    publishes: Mutex<VecDeque<Commit>>,
}

impl Shares {
    pub(super) fn creates(&self, script: &[Commit]) {
        self.0.creates.lock().unwrap().extend(script);
    }

    pub(super) fn publishes(&self, script: &[Commit]) {
        self.0.publishes.lock().unwrap().extend(script);
    }

    /// Every share the store holds for `owner`.
    pub(super) async fn of(&self, owner: &PrincipalId) -> Vec<Share> {
        self.0.store.owned_by(owner).await.unwrap().items().to_vec()
    }
}

impl ShareStore for Shares {
    async fn create(&self, new: NewShare) -> Result<Share, ShareStoreError> {
        let next = self.0.creates.lock().unwrap().pop_front();
        match next {
            None => self.0.store.create(new).await,
            Some(Commit::Refuse(error)) => Err(error),
            Some(Commit::WriteThenLose) => {
                self.0.store.create(new).await?;
                Err(ShareStoreError::Unavailable)
            }
        }
    }

    async fn get(&self, id: &ShareId) -> Result<Share, ShareStoreError> {
        self.0.store.get(id).await
    }

    async fn publish(
        &self,
        id: &ShareId,
        version: NewVersion,
    ) -> Result<ShareVersion, ShareStoreError> {
        let next = self.0.publishes.lock().unwrap().pop_front();
        match next {
            None => self.0.store.publish(id, version).await,
            Some(Commit::Refuse(error)) => Err(error),
            Some(Commit::WriteThenLose) => {
                self.0.store.publish(id, version).await?;
                Err(ShareStoreError::Unavailable)
            }
        }
    }

    async fn version(
        &self,
        id: &ShareId,
        number: ShareVersion,
    ) -> Result<SharedVersion, ShareStoreError> {
        self.0.store.version(id, number).await
    }

    async fn owned_by(&self, owner: &PrincipalId) -> Result<Listing<Share>, ShareStoreError> {
        self.0.store.owned_by(owner).await
    }
}

/// What one put does in place of storing.
#[derive(Debug, Clone, Copy)]
pub(super) enum Put {
    Refuse,
    Hang,
}

/// What every delete does.
#[derive(Debug, Clone, Copy, Default)]
pub(super) enum Delete {
    #[default]
    Pass,
    Fail,
    Hang,
}

/// A file store over memory whose `n`th put and every delete follow a
/// script.
#[derive(Clone, Default)]
pub(super) struct Files(Arc<FilesInner>);

#[derive(Default)]
pub(super) struct FilesInner {
    store: MemoryFileStore,
    puts: AtomicUsize,
    put_at: Mutex<Option<(usize, Put)>>,
    deletes: Mutex<Delete>,
}

impl Files {
    /// Makes the `nth` put, counted from 1, do `put`.
    pub(super) fn put_at(&self, nth: usize, put: Put) {
        *self.0.put_at.lock().unwrap() = Some((nth, put));
    }

    pub(super) fn deletes(&self, delete: Delete) {
        *self.0.deletes.lock().unwrap() = delete;
    }

    /// Stamps every later put with `time`.
    pub(super) fn set_time(&self, time: flare_core::Timestamp) {
        self.0.store.set_time(time);
    }

    /// Every stored file, as its share and id.
    pub(super) async fn stored(&self) -> Vec<(ShareId, FileId)> {
        let page = self.0.store.list(None, 1000).await.unwrap();
        page.files
            .iter()
            .map(|file| (file.share().clone(), file.id().clone()))
            .collect()
    }

    pub(super) async fn read(&self, share: &ShareId, id: &FileId) -> Option<StoredFile> {
        self.0.store.get(share, id).await.ok()
    }
}

impl FileStore for Files {
    async fn put(&self, file: NewFile) -> Result<(), FileStoreError> {
        let nth = self.0.puts.fetch_add(1, Ordering::SeqCst) + 1;
        let scripted = *self.0.put_at.lock().unwrap();
        match scripted {
            Some((at, Put::Refuse)) if at == nth => Err(FileStoreError::Unavailable),
            Some((at, Put::Hang)) if at == nth => std::future::pending().await,
            _ => self.0.store.put(file).await,
        }
    }

    async fn get(&self, share: &ShareId, id: &FileId) -> Result<StoredFile, FileStoreError> {
        self.0.store.get(share, id).await
    }

    async fn delete(&self, share: &ShareId, id: &FileId) -> Result<(), FileStoreError> {
        let delete = *self.0.deletes.lock().unwrap();
        match delete {
            Delete::Pass => self.0.store.delete(share, id).await,
            Delete::Fail => Err(FileStoreError::Unavailable),
            Delete::Hang => std::future::pending().await,
        }
    }

    async fn list(
        &self,
        after: Option<&FileCursor>,
        limit: usize,
    ) -> Result<FilePage, FileStoreError> {
        self.0.store.list(after, limit).await
    }
}

/// A creators rule that lets each person create `times` times, and then
/// no more: a creator who loses the right between the check and the write.
pub(super) struct Lapsing(pub(super) AtomicUsize);

impl Creators for Lapsing {
    async fn may_create(&self, _who: &Principal) -> Result<bool, CreatorsError> {
        let left = self.0.load(Ordering::SeqCst);
        if left == 0 {
            return Ok(false);
        }
        self.0.store(left - 1, Ordering::SeqCst);
        Ok(true)
    }
}

/// A clock whose every read is `step` later than the one before.
pub(super) struct Steps {
    start: Instant,
    reads: AtomicUsize,
    step: Duration,
}

impl Steps {
    pub(super) fn new(step: Duration) -> Self {
        Self {
            start: Instant::now(),
            reads: AtomicUsize::new(0),
            step,
        }
    }
}

impl Clock for Steps {
    fn now(&self) -> Instant {
        let reads = self.reads.fetch_add(1, Ordering::SeqCst);
        self.start + self.step * u32::try_from(reads).unwrap()
    }

    fn wall(&self) -> SystemTime {
        SystemTime::now()
    }
}

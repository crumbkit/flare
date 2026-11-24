//! A response store over the memory store that counts what reads hand to
//! a caller, and fails index writes while a test holds a switch on.

use flare_core::memory::MemoryResponseStore;
use flare_core::{
    IndexEntry, NewEntry, NewLink, Position, Reader, ResponseLink, ResponseStore,
    ResponseStoreError, ShareId,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The memory store, with a count of the links and entries that
/// `to_flare` and `visible` returned, and switches that fail `index` and
/// `unindex`.
#[derive(Debug, Default)]
pub(crate) struct Instrumented {
    /// The store that answers.
    pub(crate) inner: MemoryResponseStore,
    /// The links and entries that flare reads returned since the last
    /// [`Instrumented::take_examined`].
    examined: AtomicUsize,
    /// Fails `index` while on.
    pub(crate) fail_index: AtomicBool,
    /// Fails `unindex` while on.
    pub(crate) fail_unindex: AtomicBool,
}

impl Instrumented {
    /// The count of links and entries returned since the last call, which
    /// it resets.
    pub(crate) fn take_examined(&self) -> usize {
        self.examined.swap(0, Ordering::SeqCst)
    }

    fn count(&self, read: usize) {
        self.examined.fetch_add(read, Ordering::SeqCst);
    }
}

impl ResponseStore for Instrumented {
    async fn link(&self, link: NewLink) -> Result<ResponseLink, ResponseStoreError> {
        self.inner.link(link).await
    }

    async fn head(&self) -> Result<Position, ResponseStoreError> {
        self.inner.head().await
    }

    async fn to_flare(
        &self,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> Result<Vec<ResponseLink>, ResponseStoreError> {
        let read = self.inner.to_flare(flare, after, upto, limit).await?;
        self.count(read.len());
        Ok(read)
    }

    async fn of_response(
        &self,
        response: &ShareId,
    ) -> Result<Vec<ResponseLink>, ResponseStoreError> {
        self.inner.of_response(response).await
    }

    async fn pairs(
        &self,
        responses: &[ShareId],
        flare: &ShareId,
    ) -> Result<Vec<ResponseLink>, ResponseStoreError> {
        self.inner.pairs(responses, flare).await
    }

    async fn index(&self, entries: Vec<NewEntry>) -> Result<(), ResponseStoreError> {
        if self.fail_index.load(Ordering::SeqCst) {
            return Err(ResponseStoreError::Unavailable);
        }
        self.inner.index(entries).await
    }

    async fn unindex(&self, reader: &Reader, response: &ShareId) -> Result<(), ResponseStoreError> {
        if self.fail_unindex.load(Ordering::SeqCst) {
            return Err(ResponseStoreError::Unavailable);
        }
        self.inner.unindex(reader, response).await
    }

    async fn visible(
        &self,
        reader: &Reader,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> Result<Vec<IndexEntry>, ResponseStoreError> {
        let read = self
            .inner
            .visible(reader, flare, after, upto, limit)
            .await?;
        self.count(read.len());
        Ok(read)
    }
}

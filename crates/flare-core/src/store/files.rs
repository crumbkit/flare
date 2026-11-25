//! The [`FileStore`] trait, its error, and the pages its list returns.

use crate::file::{FileId, NewFile, StoredFile};
use crate::share::ShareId;
use crate::time::Timestamp;
use std::future::Future;

/// The most files one page of [`FileStore::list`] holds.
pub const MAX_FILE_PAGE: usize = 1000;

/// One stored file as a list names it: its share, its id and when the
/// store stored it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedFile {
    share: ShareId,
    id: FileId,
    stored: Timestamp,
}

impl ListedFile {
    /// The file `id` under `share`, stored at `stored`.
    #[must_use]
    pub fn new(share: ShareId, id: FileId, stored: Timestamp) -> Self {
        Self { share, id, stored }
    }

    /// The share it is stored under.
    #[must_use]
    pub fn share(&self) -> &ShareId {
        &self.share
    }

    /// Its id.
    #[must_use]
    pub fn id(&self) -> &FileId {
        &self.id
    }

    /// When the store stored it.
    #[must_use]
    pub fn stored(&self) -> Timestamp {
        self.stored
    }
}

/// Where the next page of a list starts: text that only the store that
/// wrote it reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCursor(String);

impl FileCursor {
    /// The cursor that a store writes as `text`.
    #[must_use]
    pub fn new(text: String) -> Self {
        Self(text)
    }

    /// The cursor's text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One page of a list: the files in the store's order, and where the next
/// page starts when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FilePage {
    /// The files, at most the limit asked for.
    pub files: Vec<ListedFile>,
    /// Where the next page starts, or `None` on the last page.
    pub next: Option<FileCursor>,
}

impl FilePage {
    /// A page of `files`, followed by the page at `next`.
    #[must_use]
    pub fn new(files: Vec<ListedFile>, next: Option<FileCursor>) -> Self {
        Self { files, next }
    }
}

/// Why a file store refused an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FileStoreError {
    /// No file with that id is stored under that share.
    #[error("no such file")]
    NotFound,
    /// A file with that id is stored under that share. The first file is
    /// unchanged.
    #[error("a file with that id exists")]
    IdTaken,
    /// The store could not answer. After a write the outcome is unknown.
    #[error("the file store is unavailable")]
    Unavailable,
}

/// Keeps the files of shares, each under one share. A read returns every
/// write that completed before it started.
pub trait FileStore: Send + Sync {
    /// Stores a file under its share and id.
    ///
    /// # Errors
    ///
    /// Returns [`FileStoreError::IdTaken`] when the share has a file with
    /// that id, and leaves the first file unchanged, and
    /// [`FileStoreError::Unavailable`] when the store cannot answer.
    fn put(&self, file: NewFile) -> impl Future<Output = Result<(), FileStoreError>> + Send;

    /// Reads the file `id` stored under `share`. A file stored under another
    /// share is not found.
    ///
    /// # Errors
    ///
    /// Returns [`FileStoreError::NotFound`] when the share has no such file
    /// and [`FileStoreError::Unavailable`] when the store cannot answer.
    fn get(
        &self,
        share: &ShareId,
        id: &FileId,
    ) -> impl Future<Output = Result<StoredFile, FileStoreError>> + Send;

    /// Deletes the file `id` stored under `share`. Deleting a file that is
    /// not stored succeeds, so a repeated delete is harmless.
    ///
    /// # Errors
    ///
    /// Returns [`FileStoreError::Unavailable`] when the store cannot answer;
    /// the file may or may not be deleted.
    fn delete(
        &self,
        share: &ShareId,
        id: &FileId,
    ) -> impl Future<Output = Result<(), FileStoreError>> + Send;

    /// One page of every stored file, each with the time the store stored
    /// it: at most `limit` files, capped at [`MAX_FILE_PAGE`], from the
    /// start or from `after`, the cursor of the page before. Walking every
    /// page names each file stored before the walk began and not deleted
    /// during it at least once.
    ///
    /// # Errors
    ///
    /// Returns [`FileStoreError::Unavailable`] when the store cannot answer.
    fn list(
        &self,
        after: Option<&FileCursor>,
        limit: usize,
    ) -> impl Future<Output = Result<FilePage, FileStoreError>> + Send;

    /// A cursor from which [`FileStore::list`] names every file stored
    /// under `share` and under each share whose id sorts after it, and no
    /// file of a share before it. A sweep starts its walk there.
    ///
    /// The default returns `None`: such a store lists only from the start,
    /// and a sweep over it always begins at the start of the list. The
    /// stores that flare ships override it.
    fn cursor_at(&self, share: &ShareId) -> Option<FileCursor> {
        let _ = share;
        None
    }
}

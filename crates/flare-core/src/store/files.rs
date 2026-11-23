//! The [`FileStore`] trait and its error.

use crate::file::{FileId, NewFile, StoredFile};
use crate::share::ShareId;
use std::future::Future;

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
}

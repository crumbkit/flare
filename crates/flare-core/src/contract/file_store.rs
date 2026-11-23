//! The [`FileStore`] contract suite.
//!
//! Every function stores the files it needs through the store, so the
//! caller passes a new, empty store.

use super::{Clause, RunTogether, Violation, ensure, fixture, unexpected};
use crate::{
    FileBytes, FileId, FileStore, FileStoreError, MAX_FILE_BYTES, MediaType, NewFile, ShareId,
    StoredFile,
};

/// How many pairs of puts the concurrency clause runs.
const ROUNDS: u8 = 8;

/// The shares, media types and bytes that the suite uses.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FileFixture {
    share: ShareId,
    other_share: ShareId,
    png: MediaType,
    jpeg: MediaType,
    bytes: FileBytes,
    other_bytes: FileBytes,
}

impl FileFixture {
    /// The standard fixture.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by its
    /// type, which means this crate is broken.
    pub fn standard() -> Result<Self, Violation> {
        Ok(Self {
            share: ShareId::from_random([0xA0; 16]),
            other_share: ShareId::from_random([0xA1; 16]),
            png: fixture(MediaType::try_from("image/png"))?,
            jpeg: fixture(MediaType::try_from("image/jpeg"))?,
            bytes: fixture(FileBytes::try_from(b"first file".to_vec()))?,
            other_bytes: fixture(FileBytes::try_from(b"second file".to_vec()))?,
        })
    }

    fn file(&self, id: &FileId) -> NewFile {
        NewFile::new(
            self.share.clone(),
            id.clone(),
            self.png.clone(),
            self.bytes.clone(),
        )
    }

    fn other_file(&self, id: &FileId) -> NewFile {
        NewFile::new(
            self.share.clone(),
            id.clone(),
            self.jpeg.clone(),
            self.other_bytes.clone(),
        )
    }
}

/// A stored file reads back with its share, id, media type and bytes.
///
/// # Errors
///
/// Returns [`Clause::AStoredFileReadsBack`] otherwise.
pub async fn a_stored_file_reads_back<F: FileStore>(
    store: &F,
    contract_fixture: &FileFixture,
) -> Result<(), Violation> {
    let id = FileId::from_random([1; 16]);
    let file = contract_fixture.file(&id);
    unexpected(store.put(file.clone()).await)?;
    let read = store.get(&contract_fixture.share, &id).await;
    ensure(
        read == Ok(StoredFile::from(file)),
        Clause::AStoredFileReadsBack,
        || format!("the file reads as {read:?}"),
    )
}

/// A file stored under one share is not found under another share.
///
/// # Errors
///
/// Returns [`Clause::AFileIsNotFoundUnderAnotherShare`] otherwise.
pub async fn a_file_is_not_found_under_another_share<F: FileStore>(
    store: &F,
    contract_fixture: &FileFixture,
) -> Result<(), Violation> {
    let id = FileId::from_random([2; 16]);
    unexpected(store.put(contract_fixture.file(&id)).await)?;
    let read = store.get(&contract_fixture.other_share, &id).await;
    let own = store.get(&contract_fixture.share, &id).await;
    ensure(
        read == Err(FileStoreError::NotFound) && own.is_ok(),
        Clause::AFileIsNotFoundUnderAnotherShare,
        || format!("under another share the file reads as {read:?}"),
    )
}

/// Storing to a taken id answers `IdTaken` and leaves the first file
/// unchanged.
///
/// # Errors
///
/// Returns [`Clause::ATakenFileIdChangesNothing`] otherwise.
pub async fn a_taken_file_id_changes_nothing<F: FileStore>(
    store: &F,
    contract_fixture: &FileFixture,
) -> Result<(), Violation> {
    let id = FileId::from_random([3; 16]);
    let first = contract_fixture.file(&id);
    unexpected(store.put(first.clone()).await)?;
    let answer = store.put(contract_fixture.other_file(&id)).await;
    let read = store.get(&contract_fixture.share, &id).await;
    ensure(
        answer == Err(FileStoreError::IdTaken) && read == Ok(StoredFile::from(first)),
        Clause::ATakenFileIdChangesNothing,
        || format!("the second put answered {answer:?}; the file reads as {read:?}"),
    )
}

/// Two puts of one id, run together by `runner`, give one success and one
/// `IdTaken`, and the file is the winner's. The clause runs several pairs.
///
/// # Errors
///
/// Returns [`Clause::ConcurrentPutsOfOneIdGiveOneIdTaken`] otherwise.
pub async fn concurrent_puts_of_one_id_give_one_id_taken<F: FileStore>(
    store: &F,
    contract_fixture: &FileFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    for round in 0..ROUNDS {
        let id = FileId::from_random([0x40 | round; 16]);
        let first = contract_fixture.file(&id);
        let second = contract_fixture.other_file(&id);
        let answers = runner
            .run_together(|| store.put(first.clone()), || store.put(second.clone()))
            .await;
        let (one, two) = unexpected(answers.ok_or("a concurrent put panicked"))?;
        let read = unexpected(store.get(&contract_fixture.share, &id).await)?;
        let winner = match (&one, &two) {
            (Ok(()), Err(FileStoreError::IdTaken)) => Some(StoredFile::from(first)),
            (Err(FileStoreError::IdTaken), Ok(())) => Some(StoredFile::from(second)),
            _ => None,
        };
        ensure(
            winner.is_some_and(|winner| winner == read),
            Clause::ConcurrentPutsOfOneIdGiveOneIdTaken,
            || format!("round {round}: answers {one:?} and {two:?}"),
        )?;
    }
    Ok(())
}

/// An unknown file answers `NotFound`.
///
/// # Errors
///
/// Returns [`Clause::AnUnknownFileIsNotFound`] otherwise.
pub async fn an_unknown_file_is_not_found<F: FileStore>(
    store: &F,
    contract_fixture: &FileFixture,
) -> Result<(), Violation> {
    let read = store
        .get(&contract_fixture.share, &FileId::from_random([5; 16]))
        .await;
    ensure(
        read == Err(FileStoreError::NotFound),
        Clause::AnUnknownFileIsNotFound,
        || format!("an unknown file reads as {read:?}"),
    )
}

/// A file of [`MAX_FILE_BYTES`] reads back whole.
///
/// # Errors
///
/// Returns [`Clause::TheLargestFileReadsBack`] otherwise.
pub async fn the_largest_file_reads_back<F: FileStore>(
    store: &F,
    contract_fixture: &FileFixture,
) -> Result<(), Violation> {
    let id = FileId::from_random([6; 16]);
    let bytes = fixture(FileBytes::try_from(vec![0x5A; MAX_FILE_BYTES]))?;
    let file = NewFile::new(
        contract_fixture.share.clone(),
        id.clone(),
        contract_fixture.png.clone(),
        bytes,
    );
    unexpected(store.put(file.clone()).await)?;
    let read = store.get(&contract_fixture.share, &id).await;
    ensure(
        read == Ok(StoredFile::from(file)),
        Clause::TheLargestFileReadsBack,
        || {
            format!(
                "the largest file reads as {:?}",
                read.map(|file| file.bytes().as_bytes().len())
            )
        },
    )
}

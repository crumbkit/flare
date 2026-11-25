//! The [`FileStore`] contract suite.
//!
//! Every function stores the files it needs through the store, so the
//! caller passes a new, empty store.

use super::{Clause, RunTogether, Violation, ensure, fixture, unexpected};
use crate::{
    FileBytes, FileCursor, FileId, FileStore, FileStoreError, ListedFile, MAX_FILE_BYTES,
    MAX_FILE_PAGE, MediaType, NewFile, ShareId, StoredFile, Timestamp,
};
use std::time::{SystemTime, UNIX_EPOCH};

mod start;

pub use start::a_list_from_a_share_starts_at_that_share;

/// How far a store's time may differ from this machine's clock, in
/// seconds, for a store whose clock is another machine's.
const SKEW: u64 = 300;

/// The most pages the paging clause reads before it gives up.
const MAX_PAGES: usize = 64;

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

/// This machine's time, for the clauses that check a store's time.
fn wall() -> Timestamp {
    Timestamp::from_secs(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs()),
    )
}

/// Every file the store lists, read in pages of `limit`.
async fn every_listed<F: FileStore>(store: &F, limit: usize) -> Result<Vec<ListedFile>, Violation> {
    let mut listed = Vec::new();
    let mut after: Option<FileCursor> = None;
    for _ in 0..MAX_PAGES {
        let page = unexpected(store.list(after.as_ref(), limit).await)?;
        if page.files.len() > limit {
            return Err(Violation::new(
                Clause::AListPagesThroughEveryFile,
                format!("a page of limit {limit} held {} files", page.files.len()),
            ));
        }
        listed.extend(page.files);
        match page.next {
            Some(next) => after = Some(next),
            None => return Ok(listed),
        }
    }
    Err(Violation::new(
        Clause::AListPagesThroughEveryFile,
        format!("the list did not end within {MAX_PAGES} pages"),
    ))
}

/// A deleted file is not found, and the other files of its share still
/// read back.
///
/// # Errors
///
/// Returns [`Clause::ADeletedFileIsNotFoundAndOthersStay`] otherwise.
pub async fn a_deleted_file_is_not_found_and_others_stay<F: FileStore>(
    store: &F,
    contract_fixture: &FileFixture,
) -> Result<(), Violation> {
    let gone = FileId::from_random([7; 16]);
    let kept = FileId::from_random([8; 16]);
    unexpected(store.put(contract_fixture.file(&gone)).await)?;
    unexpected(store.put(contract_fixture.file(&kept)).await)?;
    let deleted = store.delete(&contract_fixture.share, &gone).await;
    let read = store.get(&contract_fixture.share, &gone).await;
    let other = store.get(&contract_fixture.share, &kept).await;
    ensure(
        deleted == Ok(())
            && read == Err(FileStoreError::NotFound)
            && other == Ok(StoredFile::from(contract_fixture.file(&kept))),
        Clause::ADeletedFileIsNotFoundAndOthersStay,
        || {
            format!(
                "the delete answered {deleted:?}; the file reads as {read:?}; the other as {other:?}"
            )
        },
    )
}

/// Deleting a file that is not stored succeeds, and deleting a file under
/// another share leaves it.
///
/// # Errors
///
/// Returns [`Clause::DeletingAMissingFileSucceeds`] otherwise.
pub async fn deleting_a_missing_file_succeeds<F: FileStore>(
    store: &F,
    contract_fixture: &FileFixture,
) -> Result<(), Violation> {
    let id = FileId::from_random([9; 16]);
    let missing = store.delete(&contract_fixture.share, &id).await;
    unexpected(store.put(contract_fixture.file(&id)).await)?;
    let elsewhere = store.delete(&contract_fixture.other_share, &id).await;
    let read = store.get(&contract_fixture.share, &id).await;
    let twice = match store.delete(&contract_fixture.share, &id).await {
        Ok(()) => store.delete(&contract_fixture.share, &id).await,
        failed => failed,
    };
    ensure(
        missing == Ok(()) && elsewhere == Ok(()) && read.is_ok() && twice == Ok(()),
        Clause::DeletingAMissingFileSucceeds,
        || {
            format!(
                "a missing delete answered {missing:?}, one under another share {elsewhere:?} leaving {read:?}, and a repeated delete {twice:?}"
            )
        },
    )
}

/// A list names each stored file once, under its share, with the time the
/// store stored it.
///
/// # Errors
///
/// Returns [`Clause::AListNamesEachFileWithItsStoreTime`] otherwise.
pub async fn a_list_names_each_file_with_its_store_time<F: FileStore>(
    store: &F,
    contract_fixture: &FileFixture,
) -> Result<(), Violation> {
    let before = wall().as_secs().saturating_sub(SKEW);
    let one = FileId::from_random([10; 16]);
    let two = FileId::from_random([11; 16]);
    unexpected(store.put(contract_fixture.file(&one)).await)?;
    let other = NewFile::new(
        contract_fixture.other_share.clone(),
        two.clone(),
        contract_fixture.png.clone(),
        contract_fixture.bytes.clone(),
    );
    unexpected(store.put(other).await)?;
    let after = wall().as_secs().saturating_add(SKEW);
    let listed = every_listed(store, MAX_FILE_PAGE).await?;
    let found = |share: &ShareId, id: &FileId| {
        listed
            .iter()
            .filter(|file| file.share() == share && file.id() == id)
            .count()
    };
    let timely = listed
        .iter()
        .all(|file| (before..=after).contains(&file.stored().as_secs()));
    ensure(
        found(&contract_fixture.share, &one) == 1
            && found(&contract_fixture.other_share, &two) == 1
            && found(&contract_fixture.share, &two) == 0
            && listed.len() == 2
            && timely,
        Clause::AListNamesEachFileWithItsStoreTime,
        || format!("the list holds {listed:?}"),
    )
}

/// A list read two files at a time names every stored file once, and a
/// deleted file drops out of the next walk.
///
/// # Errors
///
/// Returns [`Clause::AListPagesThroughEveryFile`] otherwise.
pub async fn a_list_pages_through_every_file<F: FileStore>(
    store: &F,
    contract_fixture: &FileFixture,
) -> Result<(), Violation> {
    let ids: Vec<FileId> = (0x20..0x25_u8)
        .map(|byte| FileId::from_random([byte; 16]))
        .collect();
    for id in &ids {
        unexpected(store.put(contract_fixture.file(id)).await)?;
    }
    let first = every_listed(store, 2).await?;
    let gone = ids
        .first()
        .cloned()
        .unwrap_or_else(|| FileId::from_random([0; 16]));
    unexpected(store.delete(&contract_fixture.share, &gone).await)?;
    let second = every_listed(store, 2).await?;
    let once = |listed: &[ListedFile], id: &FileId| {
        listed.iter().filter(|file| file.id() == id).count() == 1
    };
    ensure(
        first.len() == ids.len()
            && ids.iter().all(|id| once(&first, id))
            && second.len() == ids.len() - 1
            && !second.iter().any(|file| file.id() == &gone),
        Clause::AListPagesThroughEveryFile,
        || format!("the first walk named {first:?}; after a delete, {second:?}"),
    )
}

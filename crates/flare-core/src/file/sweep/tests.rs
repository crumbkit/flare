//! Unit tests of the sweep of unlisted files.

use super::{SWEEP_AGE, SweepReport, sweep_files};
use crate::contract::block_on;
use crate::file::{FileBytes, FileId, ImagePath, MediaType, NewFile, VersionImage, VersionImages};
use crate::memory::{MemoryFileStore, MemoryShareStore};
use crate::share::{DocumentText, NewShare, NewVersion, ShareContent, ShareId, ShareTitle};
use crate::store::{FileStore, FileStoreError, ShareStore};
use crate::{PrincipalId, Timestamp};

/// The time the files are stored at.
const STORED: u64 = 1_800_000_000;

fn share(byte: u8) -> ShareId {
    ShareId::from_random([byte; 16])
}

fn file(byte: u8) -> FileId {
    FileId::from_random([byte; 16])
}

fn owner() -> PrincipalId {
    PrincipalId::try_from("ana@example.org").unwrap()
}

fn doc() -> ShareContent {
    ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap())
}

fn table(file: &FileId) -> VersionImages {
    VersionImages::try_from(vec![VersionImage::new(
        ImagePath::try_from("a.png").unwrap(),
        file.clone(),
        MediaType::try_from("image/png").unwrap(),
        3,
    )])
    .unwrap()
}

/// Stores `id` under `share` at [`STORED`].
fn put(files: &MemoryFileStore, share: &ShareId, id: &FileId) {
    files.set_time(Timestamp::from_secs(STORED));
    let new = NewFile::new(
        share.clone(),
        id.clone(),
        MediaType::try_from("image/png").unwrap(),
        FileBytes::try_from(b"png".to_vec()).unwrap(),
    );
    block_on(files.put(new)).unwrap();
}

/// Creates `share` whose version 1 lists `listed`, if any.
fn create(shares: &MemoryShareStore, id: &ShareId, listed: Option<&FileId>) {
    let new = NewShare::new(
        id.clone(),
        owner(),
        ShareTitle::try_from("Notes").unwrap(),
        doc(),
        Timestamp::from_secs(STORED),
    );
    let new = match listed {
        Some(file) => new.with_images(table(file)),
        None => new,
    };
    block_on(shares.create(new)).unwrap();
}

fn minutes_after(minutes: u64) -> Timestamp {
    Timestamp::from_secs(STORED + minutes * 60)
}

fn stored(files: &MemoryFileStore, share: &ShareId, id: &FileId) -> bool {
    block_on(files.get(share, id)).is_ok()
}

#[test]
fn an_unlisted_file_is_deleted_at_31_minutes_and_kept_at_29() {
    let (shares, files) = (MemoryShareStore::new(), MemoryFileStore::new());
    create(&shares, &share(1), None);
    put(&files, &share(1), &file(1));
    let early = block_on(sweep_files(&shares, &files, minutes_after(29), 100)).unwrap();
    assert_eq!((early.examined, early.deleted), (1, 0));
    assert!(stored(&files, &share(1), &file(1)));
    let late = block_on(sweep_files(&shares, &files, minutes_after(31), 100)).unwrap();
    assert_eq!((late.examined, late.deleted), (1, 1));
    assert!(!stored(&files, &share(1), &file(1)));
}

#[test]
fn a_file_whose_share_does_not_exist_is_deleted_and_a_listed_file_of_a_share_that_exists_is_kept() {
    let (shares, files) = (MemoryShareStore::new(), MemoryFileStore::new());
    create(&shares, &share(1), Some(&file(1)));
    put(&files, &share(1), &file(1));
    put(&files, &share(2), &file(2));
    let report = block_on(sweep_files(&shares, &files, minutes_after(31), 100)).unwrap();
    assert_eq!(report.deleted, 1);
    assert!(stored(&files, &share(1), &file(1)));
    assert!(!stored(&files, &share(2), &file(2)));
}

#[test]
fn the_sweep_never_deletes_a_file_that_any_of_100_versions_lists() {
    let (shares, files) = (MemoryShareStore::new(), MemoryFileStore::new());
    let id = share(1);
    create(&shares, &id, Some(&file(0)));
    put(&files, &id, &file(0));
    for byte in 1..100_u8 {
        let version = NewVersion::new(doc(), owner(), Timestamp::from_secs(STORED))
            .with_images(table(&file(byte)));
        block_on(shares.publish(&id, version)).unwrap();
        put(&files, &id, &file(byte));
    }
    put(&files, &id, &file(200));
    let report = block_on(sweep_files(&shares, &files, minutes_after(600), 1000)).unwrap();
    assert_eq!((report.examined, report.deleted), (101, 1));
    for byte in 0..100_u8 {
        assert!(stored(&files, &id, &file(byte)), "file {byte}");
    }
    assert!(!stored(&files, &id, &file(200)));
}

#[test]
fn a_sweep_stops_at_its_limit_and_says_so_and_a_sweep_under_its_limit_does_not() {
    let (shares, files) = (MemoryShareStore::new(), MemoryFileStore::new());
    for byte in 1..=5 {
        put(&files, &share(byte), &file(byte));
    }
    let short = block_on(sweep_files(&shares, &files, minutes_after(31), 3)).unwrap();
    assert_eq!((short.examined, short.deleted), (3, 3));
    assert!(short.stopped_early);
    let rest = block_on(sweep_files(&shares, &files, minutes_after(31), 3)).unwrap();
    assert_eq!((rest.examined, rest.deleted), (2, 2));
    assert!(!rest.stopped_early);
}

/// A share store that answers every call `Unavailable`.
struct DownShares;

impl ShareStore for DownShares {
    async fn create(&self, _new: NewShare) -> Result<crate::Share, crate::ShareStoreError> {
        Err(crate::ShareStoreError::Unavailable)
    }

    async fn get(&self, _id: &ShareId) -> Result<crate::Share, crate::ShareStoreError> {
        Err(crate::ShareStoreError::Unavailable)
    }

    async fn publish(
        &self,
        _id: &ShareId,
        _version: NewVersion,
    ) -> Result<crate::ShareVersion, crate::ShareStoreError> {
        Err(crate::ShareStoreError::Unavailable)
    }

    async fn version(
        &self,
        _id: &ShareId,
        _number: crate::ShareVersion,
    ) -> Result<crate::SharedVersion, crate::ShareStoreError> {
        Err(crate::ShareStoreError::Unavailable)
    }

    async fn owned_by(
        &self,
        _owner: &PrincipalId,
    ) -> Result<crate::Listing<crate::Share>, crate::ShareStoreError> {
        Err(crate::ShareStoreError::Unavailable)
    }
}

#[test]
fn a_share_store_that_does_not_answer_leaves_every_file_undecided() {
    let files = MemoryFileStore::new();
    put(&files, &share(1), &file(1));
    let report = block_on(sweep_files(&DownShares, &files, minutes_after(31), 100)).unwrap();
    assert_eq!(
        report,
        SweepReport {
            examined: 1,
            deleted: 0,
            undecided: 1,
            stopped_early: false,
        }
    );
    assert!(stored(&files, &share(1), &file(1)));
}

#[test]
fn the_sweep_age_is_thirty_minutes_and_a_store_that_cannot_list_is_an_error() {
    assert_eq!(SWEEP_AGE, 1800);
    struct Unlistable;
    impl FileStore for Unlistable {
        async fn put(&self, _file: NewFile) -> Result<(), FileStoreError> {
            Ok(())
        }
        async fn get(
            &self,
            _share: &ShareId,
            _id: &FileId,
        ) -> Result<crate::StoredFile, FileStoreError> {
            Err(FileStoreError::NotFound)
        }
        async fn delete(&self, _share: &ShareId, _id: &FileId) -> Result<(), FileStoreError> {
            Ok(())
        }
        async fn list(
            &self,
            _after: Option<&crate::FileCursor>,
            _limit: usize,
        ) -> Result<crate::FilePage, FileStoreError> {
            Err(FileStoreError::Unavailable)
        }
    }
    let shares = MemoryShareStore::new();
    assert_eq!(
        block_on(sweep_files(&shares, &Unlistable, minutes_after(31), 10)),
        Err(FileStoreError::Unavailable)
    );
}

//! Unit tests of an upload's writes: what it stores, what it deletes at
//! each crash point, and what the sweep later finds.

mod stores;

use super::super::Api;
use super::super::files::NoFiles;
use super::super::{ReadRefusal, Sharing};
use super::prepare::tests::png;
use super::{COMMIT_WINDOW, ImagePart, Upload, Uploaded};
use crate::clock::{Clock, SystemClock};
use flare_auth_oidc::{Admit, AllowEntry, AllowList, Reach};
use flare_contract::{ApiCode, Format, ShareRequest, Shared, VersionRequest};
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemoryTokenStore,
};
use flare_core::{
    AllAdmitted, ByIdentity, Creators, Email, FileId, FileStore as _, Flares, IdentityKind,
    NoTeams, Principal, PrincipalId, ShareId, ShareStore as _, ShareStoreError, ShareVersion,
    SweepReport, Timestamp, sweep_files,
};
use hyper::body::Bytes;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
use stores::{Commit, Delete, Files, Lapsing, Put, Shares, Steps};

/// The wall-clock time of every upload and every stored file.
const T0: u64 = 1_800_000_000;

const ANA: &str = "ana@example.org";
const BOB: &str = "bob@example.org";

fn person(address: &str) -> Principal {
    let email = Email::try_from(address).unwrap();
    Principal::with_email(email.principal_id(), email)
}

fn admit() -> Admit {
    let entries = [ANA, BOB]
        .iter()
        .map(|address| {
            AllowEntry::new(
                Email::try_from(*address).unwrap().into(),
                PrincipalId::try_from(*address).unwrap(),
                Reach::Everything,
            )
        })
        .collect();
    Admit::Emails(AllowList::new(entries).unwrap())
}

type Site<K> = Sharing<
    Shares,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
    MemoryTokenStore,
    ByIdentity,
    NoTeams,
    K,
    Files,
>;

/// The share API over `shares` and `files`, with `creators`.
fn site<K: Creators>(shares: &Shares, files: &Files, creators: K) -> Site<K> {
    let flares = Flares::new(
        shares.clone(),
        MemoryGrantStore::new(),
        MemoryResponseStore::new(),
        MemoryCommentStore::new(),
        MemoryCursorStore::new(),
        creators,
    );
    Sharing::new(
        flares,
        MemoryTokenStore::default(),
        ByIdentity(IdentityKind::Email),
        admit(),
    )
    .with_files(files.clone())
}

/// Stores whose files are stamped at [`T0`].
fn stores() -> (Shares, Files) {
    let files = Files::default();
    files.set_time(Timestamp::from_secs(T0));
    (Shares::default(), files)
}

/// A document that shows each of `paths`.
fn showing(paths: &[&str]) -> String {
    let mut text = String::from("# Notes\n\n");
    for path in paths {
        text.push_str(&format!("![an image]({path})\n\n"));
    }
    text
}

/// One PNG part for each of `paths`.
fn parts(paths: &[&str]) -> Vec<ImagePart> {
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| ImagePart {
            path: (*path).to_owned(),
            bytes: Bytes::from(png(u32::try_from(index).unwrap() + 2)),
        })
        .collect()
}

fn create(paths: &[&str]) -> Upload {
    let request = ShareRequest {
        title: "Notes".to_owned(),
        format: Format::Markdown,
        content: showing(paths),
        with: Vec::new(),
        re: Vec::new(),
    };
    Upload::Create(request, parts(paths))
}

fn publish(id: &ShareId, paths: &[&str]) -> Upload {
    let request = VersionRequest {
        format: Format::Markdown,
        content: showing(paths),
    };
    Upload::Publish(id.as_str().to_owned(), request, parts(paths))
}

const FIVE: [&str; 5] = ["img/a.png", "b.png", "c.png", "d.png", "e.png"];
const TWO: [&str; 2] = ["img/a.png", "b.png"];

async fn run<K: Creators>(
    site: &Site<K>,
    who: &str,
    upload: Upload,
    clock: &dyn Clock,
) -> Uploaded {
    site.upload(&person(who), upload, Timestamp::from_secs(T0), clock)
        .await
}

/// The code an upload was refused with.
fn code(uploaded: &Uploaded) -> Option<ApiCode> {
    uploaded.answer.as_ref().err().map(|refusal| refusal.code)
}

/// The share an upload created.
fn created(uploaded: &Uploaded) -> ShareId {
    let (status, body) = uploaded.answer.as_ref().unwrap();
    assert_eq!(*status, 201);
    let shared: Shared = serde_json::from_str(body.as_deref().unwrap()).unwrap();
    ShareId::try_from(shared.share.id.as_str()).unwrap()
}

async fn sweep(shares: &Shares, files: &Files, minutes: u64) -> SweepReport {
    let first = ShareId::from_random([0; 16]);
    sweep_files(
        shares,
        files,
        Timestamp::from_secs(T0 + minutes * 60),
        100,
        &first,
    )
    .await
    .unwrap()
}

fn ana() -> PrincipalId {
    PrincipalId::try_from(ANA).unwrap()
}

#[tokio::test]
async fn a_create_with_two_images_stores_each_stripped_and_version_one_lists_each_by_its_path() {
    let (shares, files) = stores();
    let site = site(&shares, &files, AllAdmitted);
    let uploaded = run(&site, ANA, create(&TWO), &SystemClock).await;
    let id = created(&uploaded);
    let version = shares.version(&id, ShareVersion::first()).await.unwrap();
    let rows = version.images().rows();
    let paths: Vec<&str> = rows.iter().map(|row| row.path().as_str()).collect();
    assert_eq!(paths, TWO);
    for (row, part) in rows.iter().zip(parts(&TWO)) {
        let stored = files.read(&id, row.file()).await.unwrap();
        let cleaned = crate::images::clean(&part.bytes).unwrap();
        assert_eq!(stored.bytes().as_bytes(), cleaned.bytes.as_slice());
        assert_eq!(stored.media_type().as_str(), "image/png");
        assert_eq!(row.size(), u32::try_from(cleaned.bytes.len()).unwrap());
    }
    assert_eq!(files.stored().await.len(), 2);
    assert!(!uploaded.cleanup_failed);
}

#[tokio::test]
async fn a_publish_with_an_image_lists_it_on_the_new_version_and_version_one_keeps_none() {
    let (shares, files) = stores();
    let site = site(&shares, &files, AllAdmitted);
    let id = created(&run(&site, ANA, create(&[]), &SystemClock).await);
    let uploaded = run(&site, ANA, publish(&id, &["img/a.png"]), &SystemClock).await;
    assert_eq!(uploaded.answer.as_ref().unwrap().0, 201);
    let first = shares.version(&id, ShareVersion::first()).await.unwrap();
    assert!(first.images().is_empty());
    let second = shares
        .version(&id, ShareVersion::try_from(2).unwrap())
        .await
        .unwrap();
    assert_eq!(second.images().rows().len(), 1);
    assert_eq!(files.stored().await.len(), 1);
}

#[tokio::test]
async fn a_refused_image_writes_nothing_and_its_admitted_twin_writes_its_share() {
    let (shares, files) = stores();
    let site = site(&shares, &files, AllAdmitted);
    let Upload::Create(request, mut images) = create(&["a.png"]) else {
        panic!("create builds a create")
    };
    images[0].bytes = Bytes::from_static(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
    let refused = run(&site, ANA, Upload::Create(request, images), &SystemClock).await;
    assert_eq!(code(&refused), Some(ApiCode::ImageFormatRefused));
    assert!(files.stored().await.is_empty());
    assert!(shares.of(&ana()).await.is_empty());
    created(&run(&site, ANA, create(&["a.png"]), &SystemClock).await);
    assert_eq!(files.stored().await.len(), 1);
}

#[tokio::test]
async fn a_person_who_may_not_create_is_refused_before_any_file_is_written_and_a_creator_is_not() {
    let (shares, files) = stores();
    let refused = run(
        &site(&shares, &files, Lapsing(AtomicUsize::new(0))),
        ANA,
        create(&TWO),
        &SystemClock,
    )
    .await;
    assert_eq!(code(&refused), Some(ApiCode::NotACreator));
    assert!(files.stored().await.is_empty());
    let admitted = run(
        &site(&shares, &files, Lapsing(AtomicUsize::new(2))),
        ANA,
        create(&TWO),
        &SystemClock,
    )
    .await;
    created(&admitted);
}

/// `upload` with each image's bytes replaced by an SVG, which the image
/// check refuses.
fn with_svg(upload: Upload) -> Upload {
    let svg = Bytes::from_static(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
    let replaced = |parts: Vec<ImagePart>| -> Vec<ImagePart> {
        parts
            .into_iter()
            .map(|part| ImagePart {
                path: part.path,
                bytes: svg.clone(),
            })
            .collect()
    };
    match upload {
        Upload::Create(request, parts) => Upload::Create(request, replaced(parts)),
        Upload::Publish(id, request, parts) => Upload::Publish(id, request, replaced(parts)),
    }
}

#[tokio::test]
async fn the_right_to_create_or_publish_is_checked_before_any_image_is_read() {
    let (shares, files) = stores();
    let no_creator = site(&shares, &files, Lapsing(AtomicUsize::new(0)));
    let refused = run(&no_creator, ANA, with_svg(create(&TWO)), &SystemClock).await;
    assert_eq!(code(&refused), Some(ApiCode::NotACreator));
    let site = site(&shares, &files, AllAdmitted);
    let id = created(&run(&site, ANA, create(&[]), &SystemClock).await);
    let reader = run(&site, BOB, with_svg(publish(&id, &TWO)), &SystemClock).await;
    assert_eq!(code(&reader), Some(ApiCode::ShareNotFound));
    let owner = run(&site, ANA, with_svg(publish(&id, &TWO)), &SystemClock).await;
    assert_eq!(code(&owner), Some(ApiCode::ImageFormatRefused));
}

#[tokio::test]
async fn a_person_who_may_not_publish_is_refused_before_any_file_is_written_and_the_owner_is_not() {
    let (shares, files) = stores();
    let site = site(&shares, &files, AllAdmitted);
    let id = created(&run(&site, ANA, create(&[]), &SystemClock).await);
    let refused = run(&site, BOB, publish(&id, &TWO), &SystemClock).await;
    assert_eq!(code(&refused), Some(ApiCode::ShareNotFound));
    assert!(files.stored().await.is_empty());
    let published = run(&site, ANA, publish(&id, &TWO), &SystemClock).await;
    assert_eq!(published.answer.unwrap().0, 201);
    assert_eq!(files.stored().await.len(), 2);
}

#[tokio::test]
async fn a_put_that_fails_at_the_third_of_five_deletes_the_two_sent_and_writes_no_share() {
    let (shares, files) = stores();
    files.put_at(3, Put::Refuse);
    let uploaded = run(
        &site(&shares, &files, AllAdmitted),
        ANA,
        create(&FIVE),
        &SystemClock,
    )
    .await;
    assert_eq!(code(&uploaded), Some(ApiCode::SharesUnavailable));
    assert!(files.stored().await.is_empty());
    assert!(shares.of(&ana()).await.is_empty());
    assert!(!uploaded.cleanup_failed);
}

#[tokio::test]
async fn a_taken_share_id_deletes_the_first_draws_files_and_draws_again() {
    let (shares, files) = stores();
    shares.creates(&[Commit::Refuse(ShareStoreError::IdTaken)]);
    let uploaded = run(
        &site(&shares, &files, AllAdmitted),
        ANA,
        create(&TWO),
        &SystemClock,
    )
    .await;
    let id = created(&uploaded);
    let stored = files.stored().await;
    assert_eq!(stored.len(), 2);
    assert!(stored.iter().all(|(share, _)| *share == id));
    assert_eq!(shares.of(&ana()).await.len(), 1);
}

#[tokio::test]
async fn a_share_whose_id_is_taken_three_times_is_refused_with_no_file_left() {
    let (shares, files) = stores();
    shares.creates(&[Commit::Refuse(ShareStoreError::IdTaken); 3]);
    let uploaded = run(
        &site(&shares, &files, AllAdmitted),
        ANA,
        create(&TWO),
        &SystemClock,
    )
    .await;
    assert_eq!(code(&uploaded), Some(ApiCode::SharesUnavailable));
    assert!(files.stored().await.is_empty());
}

#[tokio::test]
async fn a_publish_refused_for_too_many_versions_deletes_the_files_it_sent() {
    let (shares, files) = stores();
    let site = site(&shares, &files, AllAdmitted);
    let id = created(&run(&site, ANA, create(&[]), &SystemClock).await);
    shares.publishes(&[Commit::Refuse(ShareStoreError::TooManyVersions)]);
    let uploaded = run(&site, ANA, publish(&id, &TWO), &SystemClock).await;
    assert_eq!(code(&uploaded), Some(ApiCode::TooManyVersions));
    assert!(files.stored().await.is_empty());
}

#[tokio::test]
async fn a_creator_who_loses_the_right_before_the_write_is_refused_and_the_files_are_deleted() {
    let (shares, files) = stores();
    let site = site(&shares, &files, Lapsing(AtomicUsize::new(1)));
    let uploaded = run(&site, ANA, create(&TWO), &SystemClock).await;
    assert_eq!(code(&uploaded), Some(ApiCode::NotACreator));
    assert!(files.stored().await.is_empty());
    assert!(shares.of(&ana()).await.is_empty());
}

#[tokio::test]
async fn an_upload_that_reaches_its_commit_at_5_minutes_is_refused_and_its_files_deleted_and_one_just_under_commits()
 {
    let (shares, files) = stores();
    let site = site(&shares, &files, AllAdmitted);
    let late = run(&site, ANA, create(&TWO), &Steps::new(COMMIT_WINDOW)).await;
    assert_eq!(code(&late), Some(ApiCode::SharesUnavailable));
    assert!(files.stored().await.is_empty());
    assert!(shares.of(&ana()).await.is_empty());
    let under = COMMIT_WINDOW - Duration::from_secs(1);
    created(&run(&site, ANA, create(&TWO), &Steps::new(under)).await);
    assert_eq!(files.stored().await.len(), 2);
}

/// A log writer that keeps every byte for the test to read.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_failed_delete_answers_the_first_refusal_and_logs_one_warning_that_names_only_ids() {
    let log = Captured::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let (shares, files) = stores();
    shares.creates(&[Commit::Refuse(ShareStoreError::TooManyShares)]);
    files.deletes(Delete::Fail);
    let site = site(&shares, &files, AllAdmitted);
    let uploaded = run(&site, ANA, create(&TWO), &SystemClock).await;
    assert_eq!(code(&uploaded), Some(ApiCode::TooManyShares));
    assert!(uploaded.cleanup_failed);
    let text = log.text();
    let warnings: Vec<&str> = text.lines().filter(|line| line.contains("WARN")).collect();
    assert_eq!(warnings.len(), 1, "{text}");
    for (share, file) in files.stored().await {
        assert!(warnings[0].contains(share.as_str()), "{text}");
        assert!(warnings[0].contains(file.as_str()), "{text}");
    }
    for private in ["img/a.png", "b.png", ANA, "Notes", "an image"] {
        assert!(!text.contains(private), "{private} in {text}");
    }
    // The twin: a delete that works counts no failure and logs nothing.
    files.deletes(Delete::Pass);
    shares.creates(&[Commit::Refuse(ShareStoreError::TooManyShares)]);
    let before = log.text().len();
    let clean = run(&site, ANA, create(&TWO), &SystemClock).await;
    assert!(!clean.cleanup_failed);
    assert!(!log.text()[before..].contains("WARN"));
}

/// Runs `upload` until it stops: a process that stops part way.
async fn stopped<K: Creators>(site: &Site<K>, upload: Upload) {
    let run = run(site, ANA, upload, &SystemClock);
    assert!(
        tokio::time::timeout(Duration::from_millis(200), run)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_process_that_stops_after_two_puts_leaves_files_the_sweep_keeps_at_29_minutes_and_deletes_at_31()
 {
    let (shares, files) = stores();
    files.put_at(3, Put::Hang);
    stopped(&site(&shares, &files, AllAdmitted), create(&FIVE)).await;
    assert_eq!(files.stored().await.len(), 2);
    assert_eq!(sweep(&shares, &files, 29).await.deleted, 0);
    assert_eq!(sweep(&shares, &files, 31).await.deleted, 2);
    assert!(files.stored().await.is_empty());
}

#[tokio::test]
async fn a_process_that_stops_while_it_deletes_leaves_files_the_sweep_keeps_at_29_minutes_and_deletes_at_31()
 {
    let (shares, files) = stores();
    shares.creates(&[Commit::Refuse(ShareStoreError::TooManyShares)]);
    files.deletes(Delete::Hang);
    stopped(&site(&shares, &files, AllAdmitted), create(&TWO)).await;
    assert_eq!(files.stored().await.len(), 2);
    files.deletes(Delete::Pass);
    assert_eq!(sweep(&shares, &files, 29).await.deleted, 0);
    assert_eq!(sweep(&shares, &files, 31).await.deleted, 2);
}

#[tokio::test]
async fn a_commit_whose_outcome_is_unknown_keeps_its_files_and_the_sweep_keeps_them_at_29_minutes_and_deletes_them_at_31()
 {
    let (shares, files) = stores();
    shares.creates(&[Commit::Refuse(ShareStoreError::Unavailable)]);
    let uploaded = run(
        &site(&shares, &files, AllAdmitted),
        ANA,
        create(&TWO),
        &SystemClock,
    )
    .await;
    assert_eq!(code(&uploaded), Some(ApiCode::SharesUnavailable));
    assert_eq!(files.stored().await.len(), 2);
    assert_eq!(sweep(&shares, &files, 29).await.deleted, 0);
    assert_eq!(sweep(&shares, &files, 31).await.deleted, 2);
}

#[tokio::test]
async fn a_commit_that_was_written_and_whose_answer_was_lost_keeps_every_file_through_the_sweep() {
    let (shares, files) = stores();
    shares.creates(&[Commit::WriteThenLose]);
    let uploaded = run(
        &site(&shares, &files, AllAdmitted),
        ANA,
        create(&TWO),
        &SystemClock,
    )
    .await;
    assert_eq!(code(&uploaded), Some(ApiCode::SharesUnavailable));
    assert_eq!(shares.of(&ana()).await.len(), 1);
    assert_eq!(sweep(&shares, &files, 600).await.deleted, 0);
    assert_eq!(files.stored().await.len(), 2);
}

#[tokio::test]
async fn a_file_reads_only_at_a_version_that_lists_it_and_every_other_read_is_the_same_not_found() {
    let (shares, files) = stores();
    let site = site(&shares, &files, AllAdmitted);
    let id = created(&run(&site, ANA, create(&TWO), &SystemClock).await);
    let version = shares.version(&id, ShareVersion::first()).await.unwrap();
    let listed = version.images().rows()[0].file().as_str().to_owned();
    let other = version.images().rows()[1].file().clone();
    let read = async |who: &str, share: &str, number: &str, file: &str| {
        site.file(&person(who), share, number, file).await
    };
    assert!(read(ANA, id.as_str(), "1", &listed).await.is_ok());
    let unlisted = FileId::from_random([7; 16]);
    files.delete(&id, &other).await.unwrap();
    for (who, share, number, file) in [
        (BOB, id.as_str(), "1", listed.as_str()),
        (ANA, id.as_str(), "1", unlisted.as_str()),
        (ANA, id.as_str(), "1", other.as_str()),
        (ANA, id.as_str(), "2", listed.as_str()),
        (ANA, id.as_str(), "x", listed.as_str()),
        (ANA, "not-a-share", "1", listed.as_str()),
    ] {
        assert_eq!(
            read(who, share, number, file).await.err(),
            Some(ReadRefusal::NotFound),
            "{who} {number} {file}"
        );
    }
}

#[tokio::test]
async fn a_site_with_no_file_store_refuses_an_image_and_writes_no_share_and_admits_a_share_with_none()
 {
    let shares = Shares::default();
    let flares = Flares::new(
        shares.clone(),
        MemoryGrantStore::new(),
        MemoryResponseStore::new(),
        MemoryCommentStore::new(),
        MemoryCursorStore::new(),
        AllAdmitted,
    );
    let site: Sharing<_, _, _, _, _, _, _, NoTeams, AllAdmitted, NoFiles> = Sharing::new(
        flares,
        MemoryTokenStore::default(),
        ByIdentity(IdentityKind::Email),
        admit(),
    );
    let refused = site
        .upload(
            &person(ANA),
            create(&TWO),
            Timestamp::from_secs(T0),
            &SystemClock,
        )
        .await;
    assert_eq!(code(&refused), Some(ApiCode::SharesUnavailable));
    assert!(shares.of(&ana()).await.is_empty());
    let admitted = site
        .upload(
            &person(ANA),
            create(&[]),
            Timestamp::from_secs(T0),
            &SystemClock,
        )
        .await;
    created(&admitted);
}

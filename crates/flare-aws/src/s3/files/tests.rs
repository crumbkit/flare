//! How the file store builds a put and reads S3's answers.

use super::{delete_failure, get_failure, put_failure};
use crate::dynamodb::retry::Sent;
use crate::s3::Bucket;
use crate::s3::keys::file_of_key;
use flare_core::{FileBytes, FileId, FileStoreError, MediaType, NewFile, ShareId};

fn file() -> NewFile {
    NewFile::new(
        ShareId::from_random([1; 16]),
        FileId::from_random([2; 16]),
        MediaType::try_from("image/png").unwrap(),
        FileBytes::try_from(b"picture".to_vec()).unwrap(),
    )
}

#[test]
fn a_put_creates_only_and_sets_the_content_type_under_the_files_key() {
    let bucket = Bucket::unreachable();
    let request = bucket.put_request(&file());
    assert_eq!(request.get_if_none_match().as_deref(), Some("*"));
    assert_eq!(request.get_content_type().as_deref(), Some("image/png"));
    assert_eq!(request.get_bucket().as_deref(), Some(bucket.name()));
    assert_eq!(
        request.get_key().as_deref(),
        Some(format!("files/{}/{}", "01".repeat(16), "02".repeat(16)).as_str())
    );
}

#[test]
fn a_taken_key_is_id_taken_and_a_conflicting_put_is_retried() {
    assert_eq!(
        put_failure(Some(412)),
        Sent::Refused(FileStoreError::IdTaken)
    );
    assert_eq!(put_failure(Some(409)), Sent::Again);
}

#[test]
fn a_put_whose_answer_is_lost_or_failed_on_the_server_has_an_unknown_outcome() {
    assert_eq!(put_failure(None), Sent::Unknown);
    assert_eq!(put_failure(Some(500)), Sent::Unknown);
    assert_eq!(put_failure(Some(503)), Sent::Unknown);
}

#[test]
fn a_refused_put_is_unavailable() {
    assert_eq!(put_failure(Some(403)), Sent::Unavailable);
    assert_eq!(put_failure(Some(400)), Sent::Unavailable);
}

#[test]
fn only_no_such_key_is_not_found_and_every_other_failed_read_is_unavailable() {
    assert_eq!(get_failure(Some(404), true), FileStoreError::NotFound);
    assert_eq!(get_failure(Some(404), false), FileStoreError::Unavailable);
    assert_eq!(get_failure(Some(403), false), FileStoreError::Unavailable);
    assert_eq!(get_failure(Some(500), false), FileStoreError::Unavailable);
    assert_eq!(get_failure(None, false), FileStoreError::Unavailable);
}

#[test]
fn a_listed_key_names_its_share_and_file_and_a_key_flare_did_not_write_names_none() {
    let (share, file) = (ShareId::from_random([1; 16]), FileId::from_random([2; 16]));
    let key = format!("files/{}/{}", share.as_str(), file.as_str());
    assert_eq!(file_of_key(&key), Some((share, file)));
    for foreign in [
        "files/x/y",
        "other/0101/0202",
        &format!("files/{}", "01".repeat(16)),
        &format!("files/{}/{}/extra", "01".repeat(16), "02".repeat(16)),
        &format!("files/{}/{}", "AB".repeat(16), "02".repeat(16)),
    ] {
        assert_eq!(file_of_key(foreign), None, "{foreign}");
    }
}

#[test]
fn a_delete_whose_answer_is_lost_is_sent_again_and_a_refused_delete_is_unavailable() {
    assert_eq!(delete_failure(None), Sent::Unknown);
    assert_eq!(delete_failure(Some(500)), Sent::Unknown);
    assert_eq!(delete_failure(Some(503)), Sent::Unknown);
    assert_eq!(delete_failure(Some(403)), Sent::Unavailable);
    assert_eq!(delete_failure(Some(400)), Sent::Unavailable);
}

#[test]
fn a_delete_names_the_files_key() {
    let bucket = Bucket::unreachable();
    let request = bucket.delete_request(
        &ShareId::from_random([1; 16]),
        &FileId::from_random([2; 16]),
    );
    assert_eq!(request.get_bucket().as_deref(), Some(bucket.name()));
    assert_eq!(
        request.get_key().as_deref(),
        Some(format!("files/{}/{}", "01".repeat(16), "02".repeat(16)).as_str())
    );
}

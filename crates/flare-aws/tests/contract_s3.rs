//! Runs every function of flare-core's file suite against `S3FileStore`
//! on the S3 stand-in, and checks how the store reads objects that flare
//! did not write.
#![cfg(test)]

#[path = "support/stand_in.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the stand-in support is test support: it panics on a broken fixture"
)]
pub mod stand_in;

use aws_sdk_s3::primitives::ByteStream;
use flare_aws::Stores;
use flare_core::contract::file_store::{self, FileFixture};
use flare_core::{FileId, FileStore, FileStoreError, MAX_FILE_BYTES, ShareId};
use stand_in::{Joined, endpoints, fresh, fresh_config, s3_client, unmade_config};

macro_rules! suite {
    ($($name:ident),* $(,)?) => {$(
        #[tokio::test(flavor = "multi_thread")]
        async fn $name() {
            let fixture = FileFixture::standard().unwrap();
            let stores = fresh().await;
            file_store::$name(&stores.files, &fixture).await.unwrap();
        }
    )*};
}

suite!(
    a_stored_file_reads_back,
    a_file_is_not_found_under_another_share,
    a_taken_file_id_changes_nothing,
    an_unknown_file_is_not_found,
    the_largest_file_reads_back,
    a_deleted_file_is_not_found_and_others_stay,
    deleting_a_missing_file_succeeds,
    a_list_names_each_file_with_its_store_time,
    a_list_pages_through_every_file,
    a_list_from_a_share_starts_at_that_share,
);

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_puts_of_one_id_give_one_id_taken() {
    let fixture = FileFixture::standard().unwrap();
    let stores = fresh().await;
    file_store::concurrent_puts_of_one_id_give_one_id_taken(&stores.files, &fixture, &Joined)
        .await
        .unwrap();
}

/// Writes an object as someone other than flare would, at the key of
/// file `id` under `share`.
async fn foreign_object(bucket: &str, share: &ShareId, id: &FileId, body: Vec<u8>, media: &str) {
    s3_client()
        .put_object()
        .bucket(bucket)
        .key(format!("files/{}/{}", share.as_str(), id.as_str()))
        .content_type(media)
        .body(ByteStream::from(body))
        .send()
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_object_with_a_media_type_flare_refuses_answers_unavailable() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    let (share, id) = (ShareId::from_random([1; 16]), FileId::from_random([2; 16]));
    foreign_object(
        config.bucket().as_str(),
        &share,
        &id,
        b"x".to_vec(),
        "not a type",
    )
    .await;
    assert_eq!(
        stores.files.get(&share, &id).await,
        Err(FileStoreError::Unavailable)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_object_over_the_largest_file_size_answers_unavailable() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    let (share, id) = (ShareId::from_random([1; 16]), FileId::from_random([3; 16]));
    let body = vec![7_u8; MAX_FILE_BYTES + 1];
    foreign_object(config.bucket().as_str(), &share, &id, body, "image/png").await;
    assert_eq!(
        stores.files.get(&share, &id).await,
        Err(FileStoreError::Unavailable)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_object_at_the_largest_file_size_reads_back() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    let (share, id) = (ShareId::from_random([1; 16]), FileId::from_random([4; 16]));
    let body = vec![7_u8; MAX_FILE_BYTES];
    foreign_object(config.bucket().as_str(), &share, &id, body, "image/png").await;
    let read = stores.files.get(&share, &id).await.unwrap();
    assert_eq!(read.bytes().as_bytes().len(), MAX_FILE_BYTES);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_bucket_answers_unavailable_and_never_not_found() {
    let stores = Stores::connect_at(&unmade_config(), &endpoints());
    let (share, id) = (ShareId::from_random([1; 16]), FileId::from_random([5; 16]));
    assert_eq!(
        stores.files.get(&share, &id).await,
        Err(FileStoreError::Unavailable)
    );
}

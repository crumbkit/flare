//! The teams document that a deployment's sync job writes to the stores
//! bucket under `teams/`, read through `Stores::teams_document` on the S3
//! stand-in.
#![cfg(test)]

#[path = "support/stand_in.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the stand-in support is test support: it panics on a broken fixture"
)]
pub mod stand_in;

use aws_sdk_s3::primitives::ByteStream;
use flare_aws::{Stores, TeamsObjectKey};
use flare_core::{
    DocumentRefused, DocumentTeams, IdentityKind, MAX_DOCUMENT_BYTES, Person, TeamName, Teams,
    TeamsDocument,
};
use stand_in::{endpoints, fresh_config, s3_client, unmade_config};

async fn put(bucket: &str, key: &str, body: Vec<u8>) {
    s3_client()
        .put_object()
        .bucket(bucket)
        .key(key)
        .content_type("application/json")
        .body(ByteStream::from(body))
        .send()
        .await
        .unwrap();
}

fn key(text: &str) -> TeamsObjectKey {
    TeamsObjectKey::try_from(text).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_teams_document_in_the_bucket_gives_the_teams_it_lists() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    let body = br#"{"teams": {"design": ["ana@example.com"]}}"#.to_vec();
    put(config.bucket().as_str(), "teams/directory.json", body).await;
    let document = stores.teams_document(key("teams/directory.json"));
    let teams = DocumentTeams::load(document, IdentityKind::Email)
        .await
        .unwrap();
    let ana = Person::try_from("ana@example.com").unwrap();
    assert_eq!(
        teams.teams_of(&ana).await.unwrap(),
        vec![TeamName::try_from("design").unwrap()]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_teams_object_is_unreadable_and_names_its_key() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    let read = stores
        .teams_document(key("teams/missing.json"))
        .read()
        .await
        .unwrap_err();
    assert!(read.reason().contains("teams/missing.json"), "{read}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_bucket_makes_the_teams_document_unreadable() {
    let stores = Stores::connect_at(&unmade_config(), &endpoints());
    let read = stores
        .teams_document(key("teams/directory.json"))
        .read()
        .await;
    assert!(read.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_teams_object_over_the_limit_is_read_only_one_byte_past_it_and_refused() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    let body = vec![b' '; MAX_DOCUMENT_BYTES + 4096];
    put(config.bucket().as_str(), "teams/big.json", body).await;
    let document = stores.teams_document(key("teams/big.json"));
    let bytes = document.read().await.unwrap();
    assert_eq!(bytes.len(), MAX_DOCUMENT_BYTES + 1);
    let refused = DocumentTeams::load(document, IdentityKind::Email)
        .await
        .unwrap_err();
    assert_eq!(refused, DocumentRefused::TooLarge);
}

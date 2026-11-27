//! Which teams a table's team grants were made under: the teams marker
//! records the source of the first start, a start under the other source is
//! refused while any team grant remains, and once the last team grant is
//! revoked the other source is recorded.
#![cfg(test)]

#[path = "support/stand_in.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the stand-in support is test support: it panics on a broken fixture"
)]
pub mod stand_in;

use aws_sdk_dynamodb::types::AttributeValue;
use flare_aws::{Stores, TeamsKind, TeamsKindError};
use flare_core::{ShareId, TeamName};
use stand_in::{dynamodb_client, endpoints, fresh_config};

const SHARE: &str = "0123456789abcdef0123456789abcdef";

fn text(value: &str) -> AttributeValue {
    AttributeValue::S(value.to_owned())
}

/// The kind the table's teams marker records, or `None` without one.
async fn marker(table: &str) -> Option<String> {
    let got = dynamodb_client()
        .get_item()
        .table_name(table)
        .key("pk", text("TEAMS"))
        .key("sk", text("TEAMS"))
        .consistent_read(true)
        .send()
        .await
        .unwrap();
    got.item().map(|item| item["kind"].as_s().unwrap().clone())
}

/// Writes the grant item of `grantee` on the share [`SHARE`].
async fn grant(table: &str, grantee: &str) {
    dynamodb_client()
        .put_item()
        .table_name(table)
        .item("pk", text(&format!("SHARE#{SHARE}")))
        .item("sk", text(&format!("G#{grantee}")))
        .item("role", text("reader"))
        .send()
        .await
        .unwrap();
}

/// Deletes the grant item of `grantee` on the share [`SHARE`].
async fn revoke(table: &str, grantee: &str) {
    dynamodb_client()
        .delete_item()
        .table_name(table)
        .key("pk", text(&format!("SHARE#{SHARE}")))
        .key("sk", text(&format!("G#{grantee}")))
        .send()
        .await
        .unwrap();
}

fn changed(recorded: TeamsKind) -> Result<(), TeamsKindError> {
    Err(TeamsKindError::Changed {
        recorded,
        share: ShareId::try_from(SHARE).unwrap(),
        team: TeamName::try_from("design").unwrap(),
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_table_records_the_teams_of_its_first_start() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    assert_eq!(stores.teams_kind(TeamsKind::Directory).await, Ok(()));
    assert_eq!(
        marker(config.table().as_str()).await.as_deref(),
        Some("directory")
    );
    assert_eq!(stores.teams_kind(TeamsKind::Directory).await, Ok(()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_table_whose_stored_teams_hold_a_grant_refuses_a_directory_start_and_writes_nothing() {
    let config = fresh_config().await;
    let table = config.table().as_str();
    let stores = Stores::connect_at(&config, &endpoints());
    assert_eq!(stores.teams_kind(TeamsKind::Stored).await, Ok(()));
    grant(table, "team:design").await;
    assert_eq!(
        stores.teams_kind(TeamsKind::Directory).await,
        changed(TeamsKind::Stored)
    );
    assert_eq!(marker(table).await.as_deref(), Some("stored"));
    assert_eq!(stores.teams_kind(TeamsKind::Stored).await, Ok(()));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_table_whose_directory_teams_hold_a_grant_refuses_a_stored_start_and_writes_nothing() {
    let config = fresh_config().await;
    let table = config.table().as_str();
    let stores = Stores::connect_at(&config, &endpoints());
    assert_eq!(stores.teams_kind(TeamsKind::Directory).await, Ok(()));
    grant(table, "team:design").await;
    assert_eq!(
        stores.teams_kind(TeamsKind::Stored).await,
        changed(TeamsKind::Directory)
    );
    assert_eq!(marker(table).await.as_deref(), Some("directory"));
}

#[tokio::test(flavor = "multi_thread")]
async fn once_the_last_team_grant_is_revoked_a_start_under_the_other_teams_records_them() {
    let config = fresh_config().await;
    let table = config.table().as_str();
    let stores = Stores::connect_at(&config, &endpoints());
    assert_eq!(stores.teams_kind(TeamsKind::Stored).await, Ok(()));
    grant(table, "team:design").await;
    grant(table, "ana@example.com").await;
    grant(table, "everyone").await;
    assert!(stores.teams_kind(TeamsKind::Directory).await.is_err());
    revoke(table, "team:design").await;
    assert_eq!(stores.teams_kind(TeamsKind::Directory).await, Ok(()));
    assert_eq!(marker(table).await.as_deref(), Some("directory"));
    assert_eq!(stores.teams_kind(TeamsKind::Stored).await, Ok(()));
    assert_eq!(marker(table).await.as_deref(), Some("stored"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_table_with_team_grants_and_no_marker_is_recorded_as_stored_teams() {
    let config = fresh_config().await;
    let table = config.table().as_str();
    let stores = Stores::connect_at(&config, &endpoints());
    grant(table, "team:design").await;
    assert_eq!(
        stores.teams_kind(TeamsKind::Directory).await,
        changed(TeamsKind::Stored)
    );
    assert_eq!(marker(table).await, None);
    assert_eq!(stores.teams_kind(TeamsKind::Stored).await, Ok(()));
    assert_eq!(marker(table).await.as_deref(), Some("stored"));
}

#[tokio::test(flavor = "multi_thread")]
async fn two_first_starts_together_agree_on_one_teams_source() {
    let config = fresh_config().await;
    let first = Stores::connect_at(&config, &endpoints());
    let second = Stores::connect_at(&config, &endpoints());
    let (one, two) = tokio::join!(
        first.teams_kind(TeamsKind::Directory),
        second.teams_kind(TeamsKind::Directory)
    );
    assert_eq!((one, two), (Ok(()), Ok(())));
    assert_eq!(
        marker(config.table().as_str()).await.as_deref(),
        Some("directory")
    );
}

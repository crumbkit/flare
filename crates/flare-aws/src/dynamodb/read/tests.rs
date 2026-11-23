//! Every read is consistent, so it sees every write that finished before
//! it started, and a failed read is unavailable and never empty.

use crate::dynamodb::Table;
use crate::dynamodb::items::ItemKind;
use crate::dynamodb::keys::{ENTRY_PREFIX, Key};
use flare_core::{PrincipalId, ShareId};

fn owner() -> PrincipalId {
    PrincipalId::try_from("owner@x.com").unwrap()
}

#[test]
fn a_get_asks_for_a_consistent_read_of_its_key() {
    let table = Table::unreachable();
    let key = Key::share(&ShareId::from_random([1; 16]));
    let request = table.get_request(&key);
    assert_eq!(request.get_consistent_read(), &Some(true));
    assert_eq!(request.get_table_name().as_deref(), Some(table.name()));
    assert_eq!(request.get_key().as_ref().map(|key| key.len()), Some(2));
}

#[test]
fn a_query_asks_for_a_consistent_read_of_one_prefix_with_a_bound() {
    let table = Table::unreachable();
    let partition = crate::dynamodb::keys::owner_partition(&owner());
    let request = table.query_request(&partition, ENTRY_PREFIX, 501);
    assert_eq!(request.get_consistent_read(), &Some(true));
    assert_eq!(
        request.get_key_condition_expression().as_deref(),
        Some("pk = :pk AND begins_with(sk, :prefix)")
    );
    assert_eq!(request.get_limit(), &Some(501));
    assert_eq!(request.get_scan_index_forward(), &Some(true));
}

#[tokio::test]
async fn a_read_that_cannot_reach_the_table_is_unavailable_and_never_empty() {
    let table = Table::unreachable();
    let key = Key::share(&ShareId::from_random([1; 16]));
    assert!(table.get(&key, ItemKind::Share).await.is_err());
    let partition = crate::dynamodb::keys::owner_partition(&owner());
    let listed = table
        .query(&partition, ENTRY_PREFIX, 501, ItemKind::OwnerEntry)
        .await;
    assert!(listed.is_err());
}

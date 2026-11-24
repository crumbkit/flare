//! An activity index write or removal whose answer is lost settles by
//! reading the markers: it answers success when every marker it meant to
//! write is present, or every marker it meant to remove is gone, and
//! unavailable when one is missing and the requests never arrive.
//!
//! `Fault::LoseEveryAnswer` sends each request and then reports its answer
//! as lost. `Fault::DropEveryRequest` reports the same without sending.
#![cfg(test)]

#[path = "support/stand_in.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the stand-in support is test support: it panics on a broken fixture"
)]
pub mod stand_in;

use flare_aws::{Fault, Stores};
use flare_core::{
    IndexEntry, NewEntry, NewLink, Position, PrincipalId, Reader, ResponseLink, ResponseStore,
    ResponseStoreError, ShareId, ShareVersion, Timestamp,
};
use stand_in::{endpoints, fresh_config};

/// A store with no fault and one with `fault`, on the same new table.
async fn pair(fault: Fault) -> (Stores, Stores) {
    let config = fresh_config().await;
    let healthy = Stores::connect_at(&config, &endpoints());
    let faulty = Stores::connect_at_with_fault(&config, &endpoints(), fault);
    (healthy, faulty)
}

fn reader(name: &str) -> Reader {
    Reader::Person(PrincipalId::try_from(format!("{name}@x.com").as_str()).unwrap())
}

fn share(tag: u8) -> ShareId {
    ShareId::from_random([tag; 16])
}

/// The link from response 2 to flare 1, written by the healthy store.
async fn link(healthy: &Stores) -> ResponseLink {
    let new = NewLink::new(
        share(2),
        share(1),
        ShareVersion::first(),
        Timestamp::from_secs(7),
    );
    healthy.responses.link(new).await.unwrap()
}

async fn listed(stores: &Stores, name: &str) -> Vec<IndexEntry> {
    stores
        .responses
        .visible(
            &reader(name),
            &share(1),
            Position::new(0),
            Position::new(u64::MAX),
            100,
        )
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_index_whose_answer_is_lost_answers_success_once_every_marker_is_present() {
    let (healthy, faulty) = pair(Fault::LoseEveryAnswer).await;
    let link = link(&healthy).await;
    // Ana's entry exists before the lost write, so settling finds one
    // marker that this write did not put there.
    healthy
        .responses
        .index(vec![NewEntry::new(reader("ana"), link.clone())])
        .await
        .unwrap();
    let entries = vec![
        NewEntry::new(reader("ana"), link.clone()),
        NewEntry::new(reader("bo"), link.clone()),
        NewEntry::new(reader("cy"), link),
    ];
    assert_eq!(faulty.responses.index(entries).await, Ok(()));
    for name in ["ana", "bo", "cy"] {
        assert_eq!(listed(&healthy, name).await.len(), 1, "{name}");
    }
    assert_eq!(healthy.responses.head().await, Ok(Position::new(4)));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_index_whose_requests_never_arrive_answers_unavailable_and_writes_nothing_new() {
    let (healthy, faulty) = pair(Fault::DropEveryRequest).await;
    let link = link(&healthy).await;
    healthy
        .responses
        .index(vec![NewEntry::new(reader("ana"), link.clone())])
        .await
        .unwrap();
    // Ana's marker is present and bo's is missing, so settling never
    // matches.
    let entries = vec![
        NewEntry::new(reader("ana"), link.clone()),
        NewEntry::new(reader("bo"), link),
    ];
    assert_eq!(
        faulty.responses.index(entries).await,
        Err(ResponseStoreError::Unavailable)
    );
    assert_eq!(listed(&healthy, "ana").await.len(), 1);
    assert!(listed(&healthy, "bo").await.is_empty());
    assert_eq!(healthy.responses.head().await, Ok(Position::new(2)));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unindex_whose_answer_is_lost_answers_success_once_the_marker_is_gone() {
    let (healthy, faulty) = pair(Fault::LoseEveryAnswer).await;
    let link = link(&healthy).await;
    healthy
        .responses
        .index(vec![NewEntry::new(reader("ana"), link)])
        .await
        .unwrap();
    assert_eq!(
        faulty.responses.unindex(&reader("ana"), &share(2)).await,
        Ok(())
    );
    assert!(listed(&healthy, "ana").await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unindex_whose_requests_never_arrive_answers_unavailable_and_keeps_the_entry() {
    let (healthy, faulty) = pair(Fault::DropEveryRequest).await;
    let link = link(&healthy).await;
    healthy
        .responses
        .index(vec![NewEntry::new(reader("ana"), link)])
        .await
        .unwrap();
    assert_eq!(
        faulty.responses.unindex(&reader("ana"), &share(2)).await,
        Err(ResponseStoreError::Unavailable)
    );
    assert_eq!(listed(&healthy, "ana").await.len(), 1);
}

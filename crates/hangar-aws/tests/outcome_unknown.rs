//! A write whose answer is lost after it was applied answers success once
//! a consistent read finds it, and a write that never arrived answers
//! unavailable and leaves nothing behind.
//!
//! `Fault::LoseEveryAnswer` sends each request and then reports its answer
//! as lost, as a timeout after the send would. `Fault::DropEveryRequest`
//! reports the same without sending. Both stores run with a short deadline.
#![cfg(test)]

#[path = "support/stand_in.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the stand-in support is test support: it panics on a broken fixture"
)]
pub mod stand_in;

use hangar_aws::{Fault, HostedConfig, Stores};
use hangar_core::{
    DocumentText, Email, FileBytes, FileId, FileStore, FileStoreError, GrantStore, MediaType,
    NewFile, Principal, Role, ShareContent, ShareError, ShareId, ShareStore, ShareStoreError,
    ShareTitle, ShareVersion, Shares, Timestamp,
};
use stand_in::{endpoints, fresh_config};

fn person(address: &str) -> (Principal, Email) {
    let email = Email::try_from(address).unwrap();
    (
        Principal::with_email(email.principal_id(), email.clone()),
        email,
    )
}

fn document(text: &str) -> ShareContent {
    ShareContent::Document(DocumentText::try_from(text).unwrap())
}

fn title() -> ShareTitle {
    ShareTitle::try_from("Plans").unwrap()
}

/// A store with no fault and one with `fault`, on the same new table and
/// bucket.
async fn pair(fault: Fault) -> (HostedConfig, Stores, Stores) {
    let config = fresh_config().await;
    let healthy = Stores::connect_at(&config, &endpoints());
    let faulty = Stores::connect_at_with_fault(&config, &endpoints(), fault);
    (config, healthy, faulty)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_create_and_a_publish_whose_answers_are_lost_answer_success() {
    let (_, healthy, faulty) = pair(Fault::LoseEveryAnswer).await;
    let shares = Shares::new(faulty.shares, faulty.grants);
    let (owner, _) = person("owner@x.com");
    let id = ShareId::from_random([1; 16]);
    let created = shares
        .create(
            &owner,
            id.clone(),
            title(),
            document("# One\n"),
            Timestamp::from_secs(5),
        )
        .await;
    assert!(created.is_ok(), "{created:?}");
    let published = shares
        .publish(&owner, &id, document("# Two\n"), Timestamp::from_secs(6))
        .await;
    let second = ShareVersion::try_from(2).unwrap();
    assert_eq!(published, Ok(second));
    let read = healthy.shares.get(&id).await.unwrap();
    assert_eq!(read.latest(), second);
    let version = healthy.shares.version(&id, second).await.unwrap();
    assert_eq!(version.content(), &document("# Two\n"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_grant_and_a_revoke_whose_answers_are_lost_answer_success() {
    let (config, healthy, faulty) = pair(Fault::LoseEveryAnswer).await;
    let (owner, _) = person("owner@x.com");
    let (_, bob) = person("bob@x.com");
    let id = ShareId::from_random([2; 16]);
    Shares::new(healthy.shares, healthy.grants)
        .create(
            &owner,
            id.clone(),
            title(),
            document("# One\n"),
            Timestamp::from_secs(5),
        )
        .await
        .unwrap();
    let shares = Shares::new(faulty.shares, faulty.grants);
    let reader = Stores::connect_at(&config, &endpoints());
    let granted = shares
        .grant(&owner, &id, &bob, Role::Reader, Timestamp::from_secs(6))
        .await;
    assert!(granted.is_ok(), "{granted:?}");
    let held = reader.grants.get(&id, &bob.principal_id()).await.unwrap();
    assert_eq!(held.map(|grant| grant.role()), Some(Role::Reader));
    let revoked = shares.revoke(&owner, &id, &bob).await;
    assert_eq!(revoked, Ok(()));
    assert_eq!(reader.grants.get(&id, &bob.principal_id()).await, Ok(None));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lost_grant_answer_is_settled_by_the_grants_rev_and_leaves_one_grant() {
    let (config, healthy, faulty) = pair(Fault::LoseEveryAnswer).await;
    let (owner, _) = person("owner@x.com");
    let (_, bob) = person("bob@x.com");
    let id = ShareId::from_random([3; 16]);
    Shares::new(healthy.shares, healthy.grants)
        .create(
            &owner,
            id.clone(),
            title(),
            document("# One\n"),
            Timestamp::from_secs(5),
        )
        .await
        .unwrap();
    Shares::new(faulty.shares, faulty.grants)
        .grant(&owner, &id, &bob, Role::Admin, Timestamp::from_secs(6))
        .await
        .unwrap();
    let reader = Stores::connect_at(&config, &endpoints());
    let grants = reader.grants.of_share(&id).await.unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].role(), Role::Admin);
    let listed = reader.grants.granted_to(&bob.principal_id()).await.unwrap();
    assert_eq!(listed.items().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_put_whose_answer_is_lost_answers_success() {
    let (_, healthy, faulty) = pair(Fault::LoseEveryAnswer).await;
    let file = NewFile::new(
        ShareId::from_random([4; 16]),
        FileId::from_random([5; 16]),
        MediaType::try_from("image/png").unwrap(),
        FileBytes::try_from(b"picture".to_vec()).unwrap(),
    );
    assert_eq!(faulty.files.put(file.clone()).await, Ok(()));
    let read = healthy.files.get(file.share(), file.id()).await.unwrap();
    assert_eq!(read.bytes(), file.bytes());
}

#[tokio::test(flavor = "multi_thread")]
async fn writes_that_never_arrive_answer_unavailable_and_write_nothing() {
    let (_, healthy, faulty) = pair(Fault::DropEveryRequest).await;
    let (owner, _) = person("owner@x.com");
    let (_, bob) = person("bob@x.com");
    let id = ShareId::from_random([6; 16]);
    let dropping = Shares::new(faulty.shares, faulty.grants);
    let created = dropping
        .create(
            &owner,
            id.clone(),
            title(),
            document("# One\n"),
            Timestamp::from_secs(5),
        )
        .await;
    assert_eq!(created, Err(ShareError::Unavailable));
    assert_eq!(
        healthy.shares.get(&id).await,
        Err(ShareStoreError::NotFound)
    );

    let working = Shares::new(healthy.shares, healthy.grants);
    working
        .create(
            &owner,
            id.clone(),
            title(),
            document("# One\n"),
            Timestamp::from_secs(5),
        )
        .await
        .unwrap();
    working
        .grant(&owner, &id, &bob, Role::Reader, Timestamp::from_secs(6))
        .await
        .unwrap();
    let published = dropping
        .publish(&owner, &id, document("# Two\n"), Timestamp::from_secs(7))
        .await;
    assert_eq!(published, Err(ShareError::Unavailable));
    let granted = dropping
        .grant(&owner, &id, &bob, Role::Admin, Timestamp::from_secs(8))
        .await;
    assert_eq!(granted, Err(ShareError::Unavailable));
    let revoked = dropping.revoke(&owner, &id, &bob).await;
    assert_eq!(revoked, Err(ShareError::Unavailable));
    let opened = working.open(&owner, &id, None).await.unwrap();
    assert_eq!(opened.share.latest(), ShareVersion::first());
    let grants = working.grants(&owner, &id).await.unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].role(), Role::Reader);

    let file = NewFile::new(
        id.clone(),
        FileId::from_random([7; 16]),
        MediaType::try_from("image/png").unwrap(),
        FileBytes::try_from(b"picture".to_vec()).unwrap(),
    );
    assert_eq!(
        faulty.files.put(file.clone()).await,
        Err(FileStoreError::Unavailable)
    );
}

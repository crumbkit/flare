//! The admin's authority is checked in the transaction that writes, so a
//! revoke of the admin that lands between the store's read and its write
//! refuses the write. These tests build an attempt, change the table, and
//! then send what was built, on the DynamoDB stand-in.

use super::{DynamoGrantStore, GrantOp, RevokeOp};
use crate::dynamodb::Table;
use crate::dynamodb::retry::{Attempt, Final, Policy, Sent, Token, run};
use flare_core::{
    GrantStore, GrantStoreError, Granter, PrincipalId, Revoker, Role, ShareId, Timestamp,
};

fn person(text: &str) -> PrincipalId {
    PrincipalId::try_from(text).unwrap()
}

fn owner() -> Granter {
    Granter::Owner(person("owner@x.com"))
}

/// A grant by `by` to `grantee`, from `expected`.
fn grant<'a>(
    table: &'a Table,
    share: &ShareId,
    grantee: &str,
    role: Role,
    expected: Option<Role>,
    by: Granter,
) -> GrantOp<'a> {
    GrantOp {
        table,
        share: share.clone(),
        grantee: person(grantee),
        role,
        expected,
        by,
        created: Timestamp::from_secs(10),
    }
}

fn revoke<'a>(
    table: &'a Table,
    share: &ShareId,
    grantee: &str,
    expected: Role,
    by: Revoker,
) -> RevokeOp<'a> {
    RevokeOp {
        table,
        share: share.clone(),
        grantee: person(grantee),
        expected,
        by,
    }
}

/// Runs `op` to its answer under the stores' policy.
async fn apply<A: Attempt>(op: &A) -> Final<A::Done, A::Refused> {
    run(&Policy::STORE, op).await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_grant_built_while_the_admin_held_their_grant_is_refused_once_they_are_revoked() {
    let table = Table::fresh_on_stand_in().await;
    let store = DynamoGrantStore::from_table(table.clone());
    let share = ShareId::from_random([1; 16]);
    let admin = Granter::Admin(person("admin@x.com"));
    let given = grant(&table, &share, "admin@x.com", Role::Admin, None, owner());
    assert!(matches!(apply(&given).await, Final::Done(_)));

    let racing = grant(&table, &share, "carol@x.com", Role::Reader, None, admin);
    let Ok(built) = racing.build().await else {
        panic!("the admin held their grant when the attempt was built");
    };
    let end = revoke(
        &table,
        &share,
        "admin@x.com",
        Role::Admin,
        Revoker::Owner(person("owner@x.com")),
    );
    assert_eq!(apply(&end).await, Final::Done(()));

    let sent = racing.send(&built, &Token::fresh().unwrap()).await;
    assert_eq!(sent, Sent::Refused(GrantStoreError::StaleAuthority));
    assert_eq!(store.get(&share, &person("carol@x.com")).await, Ok(None));
    assert!(
        store
            .granted_to(&person("carol@x.com"))
            .await
            .unwrap()
            .items()
            .is_empty()
    );
    assert!(store.of_share(&share).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_grant_built_while_the_admin_held_their_grant_applies_while_they_still_hold_it() {
    let table = Table::fresh_on_stand_in().await;
    let store = DynamoGrantStore::from_table(table.clone());
    let share = ShareId::from_random([2; 16]);
    let admin = Granter::Admin(person("admin@x.com"));
    let given = grant(&table, &share, "admin@x.com", Role::Admin, None, owner());
    assert!(matches!(apply(&given).await, Final::Done(_)));

    let racing = grant(&table, &share, "carol@x.com", Role::Reader, None, admin);
    let Ok(built) = racing.build().await else {
        panic!("the admin held their grant when the attempt was built");
    };
    let sent = racing.send(&built, &Token::fresh().unwrap()).await;
    assert!(matches!(sent, Sent::Done(_)), "{sent:?}");
    let carol = store.get(&share, &person("carol@x.com")).await.unwrap();
    assert_eq!(
        carol.map(|grant| grant.granted_by().clone()),
        Some(person("admin@x.com"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_revoke_built_while_the_admin_held_their_grant_is_refused_once_they_are_revoked() {
    let table = Table::fresh_on_stand_in().await;
    let store = DynamoGrantStore::from_table(table.clone());
    let share = ShareId::from_random([3; 16]);
    for (grantee, role) in [("admin@x.com", Role::Admin), ("carol@x.com", Role::Reader)] {
        let given = grant(&table, &share, grantee, role, None, owner());
        assert!(matches!(apply(&given).await, Final::Done(_)));
    }

    let racing = revoke(
        &table,
        &share,
        "carol@x.com",
        Role::Reader,
        Revoker::Admin(person("admin@x.com")),
    );
    let Ok(built) = racing.build().await else {
        panic!("the admin held their grant when the attempt was built");
    };
    let end = revoke(
        &table,
        &share,
        "admin@x.com",
        Role::Admin,
        Revoker::Owner(person("owner@x.com")),
    );
    assert_eq!(apply(&end).await, Final::Done(()));

    let sent = racing.send(&built, &Token::fresh().unwrap()).await;
    assert_eq!(sent, Sent::Refused(GrantStoreError::StaleAuthority));
    let carol = store.get(&share, &person("carol@x.com")).await.unwrap();
    assert_eq!(carol.map(|grant| grant.role()), Some(Role::Reader));
    assert_eq!(
        store
            .granted_to(&person("carol@x.com"))
            .await
            .unwrap()
            .items()
            .len(),
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_revoke_built_while_the_admin_held_their_grant_applies_while_they_still_hold_it() {
    let table = Table::fresh_on_stand_in().await;
    let store = DynamoGrantStore::from_table(table.clone());
    let share = ShareId::from_random([4; 16]);
    for (grantee, role) in [("admin@x.com", Role::Admin), ("carol@x.com", Role::Reader)] {
        let given = grant(&table, &share, grantee, role, None, owner());
        assert!(matches!(apply(&given).await, Final::Done(_)));
    }
    let racing = revoke(
        &table,
        &share,
        "carol@x.com",
        Role::Reader,
        Revoker::Admin(person("admin@x.com")),
    );
    let Ok(built) = racing.build().await else {
        panic!("the admin held their grant when the attempt was built");
    };
    assert_eq!(
        racing.send(&built, &Token::fresh().unwrap()).await,
        Sent::Done(())
    );
    assert_eq!(store.get(&share, &person("carol@x.com")).await, Ok(None));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_revoke_and_a_regrant_of_the_same_role_between_the_read_and_the_write_make_the_store_read_again()
 {
    let table = Table::fresh_on_stand_in().await;
    let store = DynamoGrantStore::from_table(table.clone());
    let share = ShareId::from_random([5; 16]);
    let given = grant(&table, &share, "bob@x.com", Role::Reader, None, owner());
    assert!(matches!(apply(&given).await, Final::Done(_)));

    let promote = grant(
        &table,
        &share,
        "bob@x.com",
        Role::Admin,
        Some(Role::Reader),
        owner(),
    );
    let Ok(built) = promote.build().await else {
        panic!("bob held a Reader grant when the attempt was built");
    };
    let end = revoke(
        &table,
        &share,
        "bob@x.com",
        Role::Reader,
        Revoker::Owner(person("owner@x.com")),
    );
    assert_eq!(apply(&end).await, Final::Done(()));
    let again = grant(&table, &share, "bob@x.com", Role::Reader, None, owner());
    assert!(matches!(apply(&again).await, Final::Done(_)));

    assert_eq!(
        promote.send(&built, &Token::fresh().unwrap()).await,
        Sent::Again
    );
    let answer = apply(&promote).await;
    assert!(
        matches!(&answer, Final::Done(grant) if grant.role() == Role::Admin),
        "{answer:?}"
    );
    let bob = store.get(&share, &person("bob@x.com")).await.unwrap();
    assert_eq!(bob.map(|grant| grant.role()), Some(Role::Admin));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_admin_who_changes_their_own_grant_needs_it_to_still_be_admin() {
    let table = Table::fresh_on_stand_in().await;
    let share = ShareId::from_random([6; 16]);
    let given = grant(&table, &share, "admin@x.com", Role::Admin, None, owner());
    assert!(matches!(apply(&given).await, Final::Done(_)));
    let admin = Granter::Admin(person("admin@x.com"));
    let demote = grant(
        &table,
        &share,
        "admin@x.com",
        Role::Reader,
        Some(Role::Admin),
        admin.clone(),
    );
    let answer = apply(&demote).await;
    assert!(
        matches!(&answer, Final::Done(grant) if grant.role() == Role::Reader),
        "{answer:?}"
    );
    let again = grant(
        &table,
        &share,
        "admin@x.com",
        Role::Admin,
        Some(Role::Reader),
        admin,
    );
    assert_eq!(
        apply(&again).await,
        Final::Refused(GrantStoreError::StaleAuthority)
    );
    let leave = revoke(
        &table,
        &share,
        "admin@x.com",
        Role::Reader,
        Revoker::Admin(person("admin@x.com")),
    );
    assert_eq!(
        apply(&leave).await,
        Final::Refused(GrantStoreError::StaleAuthority)
    );
}

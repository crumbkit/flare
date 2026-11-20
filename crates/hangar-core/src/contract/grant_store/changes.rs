//! The grant store clauses about changing a role, granting again after a
//! revoke, and leaving.

use super::{GrantFixture, grant_new, held, revoke, share, write};
use crate::contract::{Clause, Violation, ensure, unexpected};
use crate::{Grant, GrantStore, GrantStoreError, Granter, Revoker, Role, Timestamp};

/// A role change keeps the grant's time of first grant.
///
/// # Errors
///
/// Returns [`Clause::ARoleChangeKeepsTheFirstGrantTime`] otherwise.
pub async fn a_role_change_keeps_the_first_grant_time<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let id = share(6, 0);
    let bob = &contract_fixture.bob;
    grant_new(store, contract_fixture, &id, bob, Role::Reader, 10).await?;
    let by = contract_fixture.by_owner();
    let changed = unexpected(
        store
            .grant(write(&id, bob, Role::Admin, Some(Role::Reader), by, 20))
            .await,
    )?;
    let read = unexpected(store.get(&id, bob).await)?;
    let first = Timestamp::from_secs(10);
    ensure(
        changed.created() == first && read.is_some_and(|grant| grant.created() == first),
        Clause::ARoleChangeKeepsTheFirstGrantTime,
        || format!("the role change gave {changed:?}"),
    )
}

/// A revoke and a re-grant with the same role leave exactly the new grant,
/// with its own time and granter, in `get` and in both lists.
///
/// # Errors
///
/// Returns [`Clause::ARevokeAndARegrantLeaveNoStaleEntry`] otherwise.
pub async fn a_revoke_and_a_regrant_leave_no_stale_entry<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let id = share(7, 0);
    let (bob, admin) = (&contract_fixture.bob, &contract_fixture.admin);
    grant_new(store, contract_fixture, &id, admin, Role::Admin, 5).await?;
    grant_new(store, contract_fixture, &id, bob, Role::Reader, 10).await?;
    let owner = Revoker::Owner(contract_fixture.owner.clone());
    unexpected(store.revoke(revoke(&id, bob, Role::Reader, owner)).await)?;
    let by = Granter::Admin(admin.clone());
    let regrant = store
        .grant(write(&id, bob, Role::Reader, None, by, 20))
        .await;
    let expected = held(&id, bob, Role::Reader, admin, 20);
    let read = unexpected(store.get(&id, bob).await)?;
    let of_share = unexpected(store.of_share(&id).await)?;
    let granted = unexpected(store.granted_to(bob).await)?;
    let in_share: Vec<&Grant> = of_share.iter().filter(|g| g.grantee() == bob).collect();
    ensure(
        regrant.as_ref() == Ok(&expected)
            && read.as_ref() == Some(&expected)
            && in_share == [&expected]
            && granted.items() == [expected.clone()],
        Clause::ARevokeAndARegrantLeaveNoStaleEntry,
        || format!("the re-grant gave {regrant:?}; get {read:?}; person list {granted:?}"),
    )
}

/// A revoke under the grantee's own authority applies under the expected
/// role alone, for a reader and for an admin, and still answers `Changed`
/// for a role that is not current.
///
/// # Errors
///
/// Returns [`Clause::ALeaveAppliesUnderTheExpectedRoleAlone`] otherwise.
pub async fn a_leave_applies_under_the_expected_role_alone<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let id = share(9, 0);
    let (bob, admin) = (&contract_fixture.bob, &contract_fixture.admin);
    grant_new(store, contract_fixture, &id, bob, Role::Reader, 10).await?;
    grant_new(store, contract_fixture, &id, admin, Role::Admin, 10).await?;
    let wrong = store
        .revoke(revoke(&id, bob, Role::Admin, Revoker::Grantee))
        .await;
    let reader_leaves = store
        .revoke(revoke(&id, bob, Role::Reader, Revoker::Grantee))
        .await;
    let admin_leaves = store
        .revoke(revoke(&id, admin, Role::Admin, Revoker::Grantee))
        .await;
    let left = unexpected(store.of_share(&id).await)?;
    ensure(
        wrong
            == Err(GrantStoreError::Changed {
                current: Some(Role::Reader),
            })
            && reader_leaves.is_ok()
            && admin_leaves.is_ok()
            && left.is_empty(),
        Clause::ALeaveAppliesUnderTheExpectedRoleAlone,
        || format!("leaves answered {wrong:?}, {reader_leaves:?} and {admin_leaves:?}"),
    )
}

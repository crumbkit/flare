//! The [`GrantStore`] contract suite. The clauses about an admin's
//! authority, role changes and leaving, the limit, the lists and calls that
//! run together live in submodules, and this module re-exports them.
//!
//! Every function writes the grants it needs through the store, so the
//! caller passes a new, empty store.

mod authority;
mod changes;
mod groups;
mod limits;
mod lists;
mod races;

pub use authority::{
    a_grant_under_a_current_admin_is_applied, a_grant_under_a_stale_admin_is_refused,
    a_revoke_under_a_stale_admin_is_refused, an_admins_grant_never_lands_after_their_revoke,
    stale_authority_comes_before_changed,
};
pub use changes::{
    a_leave_applies_under_the_expected_role_alone, a_revoke_and_a_regrant_leave_no_stale_entry,
    a_role_change_keeps_the_first_grant_time,
};
pub use groups::a_group_grant_keeps_its_audience;
pub use limits::{concurrent_grants_at_the_limit_give_one_too_many, the_grant_limit_holds};
pub use lists::{
    a_list_shows_its_truncation_and_order, a_persons_grants_page_in_order_past_the_listing_limit,
    a_persons_list_holds_their_grants_newest_first, a_share_list_holds_only_its_grants,
};
pub use races::{
    a_grant_and_a_revoke_together_leave_a_consistent_state,
    concurrent_grants_from_no_grant_give_one_changed,
};

use super::{Clause, Violation, ensure, fixture, unexpected};
use crate::{
    Grant, GrantRevoke, GrantStore, GrantStoreError, GrantWrite, Granter, PrincipalId, Revoker,
    Role, ShareId, Timestamp,
};

/// The people and shares that the suite uses.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GrantFixture {
    owner: PrincipalId,
    admin: PrincipalId,
    bob: PrincipalId,
    carol: PrincipalId,
}

impl GrantFixture {
    /// The standard fixture.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by its
    /// type, which means this crate is broken.
    pub fn standard() -> Result<Self, Violation> {
        Ok(Self {
            owner: fixture(PrincipalId::try_from("owner@example.com"))?,
            admin: fixture(PrincipalId::try_from("admin@example.com"))?,
            bob: fixture(PrincipalId::try_from("bob@example.com"))?,
            carol: fixture(PrincipalId::try_from("carol@example.com"))?,
        })
    }

    /// The owner's authority.
    fn by_owner(&self) -> Granter {
        Granter::Owner(self.owner.clone())
    }
}

/// The share id that `tag` and `number` give, so each clause uses shares of
/// its own.
fn share(tag: u8, number: u32) -> ShareId {
    let mut bytes = [tag; 16];
    for (slot, byte) in bytes.iter_mut().skip(12).zip(number.to_be_bytes()) {
        *slot = byte;
    }
    ShareId::from_random(bytes)
}

/// The person numbered `number`.
fn person(number: usize) -> Result<PrincipalId, Violation> {
    fixture(PrincipalId::try_from(
        format!("person-{number}@example.com").as_str(),
    ))
}

/// A grant write.
fn write(
    share: &ShareId,
    grantee: &PrincipalId,
    role: Role,
    expected: Option<Role>,
    by: Granter,
    at: u64,
) -> GrantWrite {
    GrantWrite::new(
        share.clone(),
        grantee.clone(),
        role,
        expected,
        by,
        Timestamp::from_secs(at),
    )
}

/// A revoke.
fn revoke(share: &ShareId, grantee: &PrincipalId, expected: Role, by: Revoker) -> GrantRevoke {
    GrantRevoke::new(share.clone(), grantee.clone(), expected, by)
}

/// The grant that a store should hold.
fn held(share: &ShareId, grantee: &PrincipalId, role: Role, by: &PrincipalId, at: u64) -> Grant {
    Grant::new(
        share.clone(),
        grantee.clone(),
        role,
        by.clone(),
        Timestamp::from_secs(at),
    )
}

/// Grants `grantee` the role `role` from no grant, under the owner.
async fn grant_new<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
    share: &ShareId,
    grantee: &PrincipalId,
    role: Role,
    at: u64,
) -> Result<Grant, Violation> {
    unexpected(
        store
            .grant(write(
                share,
                grantee,
                role,
                None,
                contract_fixture.by_owner(),
                at,
            ))
            .await,
    )
}

/// A grant reads back with its role, its granter and its time, from
/// `grant` and from `get`.
///
/// # Errors
///
/// Returns [`Clause::AGrantReadsBack`] otherwise.
pub async fn a_grant_reads_back<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let id = share(1, 0);
    let bob = &contract_fixture.bob;
    let written = grant_new(store, contract_fixture, &id, bob, Role::Reader, 10).await?;
    let read = unexpected(store.get(&id, bob).await)?;
    let expected = held(&id, bob, Role::Reader, &contract_fixture.owner, 10);
    ensure(
        written == expected && read.as_ref() == Some(&expected),
        Clause::AGrantReadsBack,
        || format!("grant gave {written:?}, get gave {read:?}"),
    )
}

/// Granting again from the current role replaces the grant and leaves one
/// entry in each list.
///
/// # Errors
///
/// Returns [`Clause::AGrantFromTheCurrentRoleReplacesIt`] otherwise.
pub async fn a_grant_from_the_current_role_replaces_it<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let id = share(2, 0);
    let bob = &contract_fixture.bob;
    grant_new(store, contract_fixture, &id, bob, Role::Reader, 10).await?;
    let by = contract_fixture.by_owner();
    let answer = store
        .grant(write(&id, bob, Role::Admin, Some(Role::Reader), by, 20))
        .await;
    let read = unexpected(store.get(&id, bob).await)?;
    let of_share = unexpected(store.of_share(&id).await)?;
    let granted = unexpected(store.granted_to(bob).await)?;
    ensure(
        answer
            .as_ref()
            .is_ok_and(|grant| grant.role() == Role::Admin)
            && read
                .as_ref()
                .is_some_and(|grant| grant.role() == Role::Admin)
            && of_share.len() == 1
            && granted.items().len() == 1
            && granted
                .items()
                .iter()
                .all(|grant| grant.role() == Role::Admin),
        Clause::AGrantFromTheCurrentRoleReplacesIt,
        || format!("grant answered {answer:?}; get {read:?}; share list {of_share:?}"),
    )
}

/// A grant from a role that is not current answers `Changed` with the
/// current role and writes nothing.
///
/// # Errors
///
/// Returns [`Clause::AGrantFromAStaleRoleIsChanged`] otherwise.
pub async fn a_grant_from_a_stale_role_is_changed<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let id = share(3, 0);
    let (bob, carol) = (&contract_fixture.bob, &contract_fixture.carol);
    let before = grant_new(store, contract_fixture, &id, bob, Role::Reader, 10).await?;
    let by = contract_fixture.by_owner();
    let from_none = store
        .grant(write(&id, bob, Role::Admin, None, by.clone(), 20))
        .await;
    let from_reader = store
        .grant(write(&id, carol, Role::Admin, Some(Role::Reader), by, 20))
        .await;
    let bob_now = unexpected(store.get(&id, bob).await)?;
    let carol_now = unexpected(store.get(&id, carol).await)?;
    ensure(
        from_none
            == Err(GrantStoreError::Changed {
                current: Some(Role::Reader),
            })
            && from_reader == Err(GrantStoreError::Changed { current: None })
            && bob_now == Some(before)
            && carol_now.is_none(),
        Clause::AGrantFromAStaleRoleIsChanged,
        || format!("grants answered {from_none:?} and {from_reader:?}; bob {bob_now:?}"),
    )
}

/// A revoke removes the grant, and the next `get`, share list and person
/// list show it gone.
///
/// # Errors
///
/// Returns [`Clause::ARevokeIsVisibleToEveryRead`] otherwise.
pub async fn a_revoke_is_visible_to_every_read<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let id = share(4, 0);
    let bob = &contract_fixture.bob;
    grant_new(store, contract_fixture, &id, bob, Role::Reader, 10).await?;
    let owner = Revoker::Owner(contract_fixture.owner.clone());
    let answer = store.revoke(revoke(&id, bob, Role::Reader, owner)).await;
    let read = unexpected(store.get(&id, bob).await)?;
    let of_share = unexpected(store.of_share(&id).await)?;
    let granted = unexpected(store.granted_to(bob).await)?;
    ensure(
        answer.is_ok() && read.is_none() && of_share.is_empty() && granted.items().is_empty(),
        Clause::ARevokeIsVisibleToEveryRead,
        || format!("revoke answered {answer:?}; get {read:?}; person list {granted:?}"),
    )
}

/// A revoke from a role that is not current answers `Changed` and keeps the
/// grant, and a revoke of a person with no grant answers
/// `Changed { current: None }`.
///
/// # Errors
///
/// Returns [`Clause::ARevokeFromAStaleRoleIsChanged`] otherwise.
pub async fn a_revoke_from_a_stale_role_is_changed<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let id = share(5, 0);
    let (bob, carol) = (&contract_fixture.bob, &contract_fixture.carol);
    let before = grant_new(store, contract_fixture, &id, bob, Role::Admin, 10).await?;
    let owner = Revoker::Owner(contract_fixture.owner.clone());
    let stale = store
        .revoke(revoke(&id, bob, Role::Reader, owner.clone()))
        .await;
    let nobody = store.revoke(revoke(&id, carol, Role::Reader, owner)).await;
    let read = unexpected(store.get(&id, bob).await)?;
    ensure(
        stale
            == Err(GrantStoreError::Changed {
                current: Some(Role::Admin),
            })
            && nobody == Err(GrantStoreError::Changed { current: None })
            && read == Some(before),
        Clause::ARevokeFromAStaleRoleIsChanged,
        || format!("revokes answered {stale:?} and {nobody:?}; bob {read:?}"),
    )
}

/// A grant on a share the store holds no grants for is applied, and no
/// read of such a share answers an error.
///
/// # Errors
///
/// Returns [`Clause::AGrantOnAnUnseenShareIsApplied`] otherwise.
pub async fn a_grant_on_an_unseen_share_is_applied<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let clause = Clause::AGrantOnAnUnseenShareIsApplied;
    let unseen = share(8, 0);
    let bob = &contract_fixture.bob;
    let empty_get = store.get(&unseen, bob).await;
    let empty_list = store.of_share(&unseen).await;
    ensure(
        empty_get == Ok(None) && empty_list == Ok(Vec::new()),
        clause,
        || format!("an unseen share's get {empty_get:?}, list {empty_list:?}"),
    )?;
    let by = contract_fixture.by_owner();
    let answer = store
        .grant(write(&unseen, bob, Role::Reader, None, by, 10))
        .await;
    let expected = held(&unseen, bob, Role::Reader, &contract_fixture.owner, 10);
    ensure(answer == Ok(expected), clause, || {
        format!("a grant on an unseen share answered {answer:?}")
    })
}

//! The grant store clauses about writes that act under an admin.

use super::{GrantFixture, grant_new, revoke, share, write};
use crate::contract::{Clause, RunTogether, Violation, ensure, unexpected};
use crate::{GrantStore, GrantStoreError, Granter, Revoker, Role};

/// How many pairs of calls the concurrency clause runs.
const ROUNDS: u32 = 8;

/// A grant under an admin who no longer holds the Admin grant, or who holds
/// a Reader grant, or who never held one, answers `StaleAuthority` and
/// writes nothing.
///
/// # Errors
///
/// Returns [`Clause::AGrantUnderAStaleAdminIsRefused`] otherwise.
pub async fn a_grant_under_a_stale_admin_is_refused<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let id = share(20, 0);
    let (admin, bob, carol) = (
        &contract_fixture.admin,
        &contract_fixture.bob,
        &contract_fixture.carol,
    );
    grant_new(store, contract_fixture, &id, admin, Role::Admin, 10).await?;
    grant_new(store, contract_fixture, &id, bob, Role::Reader, 10).await?;
    let owner = Revoker::Owner(contract_fixture.owner.clone());
    unexpected(store.revoke(revoke(&id, admin, Role::Admin, owner)).await)?;
    let mut answers = Vec::new();
    for acting in [admin, bob, &contract_fixture.owner] {
        let by = Granter::Admin(acting.clone());
        answers.push(
            store
                .grant(write(&id, carol, Role::Reader, None, by, 20))
                .await,
        );
    }
    let carol_now = unexpected(store.get(&id, carol).await)?;
    ensure(
        answers
            .iter()
            .all(|answer| *answer == Err(GrantStoreError::StaleAuthority))
            && carol_now.is_none(),
        Clause::AGrantUnderAStaleAdminIsRefused,
        || format!("grants under a stale admin answered {answers:?}; carol {carol_now:?}"),
    )
}

/// A grant under an admin who holds the Admin grant is applied, and names
/// the admin as its granter.
///
/// # Errors
///
/// Returns [`Clause::AGrantUnderACurrentAdminIsApplied`] otherwise.
pub async fn a_grant_under_a_current_admin_is_applied<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let id = share(21, 0);
    let (admin, carol) = (&contract_fixture.admin, &contract_fixture.carol);
    grant_new(store, contract_fixture, &id, admin, Role::Admin, 10).await?;
    let by = Granter::Admin(admin.clone());
    let answer = store
        .grant(write(&id, carol, Role::Reader, None, by, 20))
        .await;
    ensure(
        answer
            .as_ref()
            .is_ok_and(|grant| grant.granted_by() == admin && grant.role() == Role::Reader),
        Clause::AGrantUnderACurrentAdminIsApplied,
        || format!("a grant under a current admin answered {answer:?}"),
    )
}

/// A revoke under an admin who no longer holds the Admin grant answers
/// `StaleAuthority` and keeps the grant.
///
/// # Errors
///
/// Returns [`Clause::ARevokeUnderAStaleAdminIsRefused`] otherwise.
pub async fn a_revoke_under_a_stale_admin_is_refused<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let id = share(22, 0);
    let (admin, carol) = (&contract_fixture.admin, &contract_fixture.carol);
    grant_new(store, contract_fixture, &id, admin, Role::Admin, 10).await?;
    let kept = grant_new(store, contract_fixture, &id, carol, Role::Reader, 10).await?;
    let owner = Revoker::Owner(contract_fixture.owner.clone());
    unexpected(store.revoke(revoke(&id, admin, Role::Admin, owner)).await)?;
    let by = Revoker::Admin(admin.clone());
    let answer = store.revoke(revoke(&id, carol, Role::Reader, by)).await;
    let carol_now = unexpected(store.get(&id, carol).await)?;
    ensure(
        answer == Err(GrantStoreError::StaleAuthority) && carol_now == Some(kept),
        Clause::ARevokeUnderAStaleAdminIsRefused,
        || format!("the revoke answered {answer:?}; carol {carol_now:?}"),
    )
}

/// A store stops honouring an admin's authority once the admin's grant is
/// revoked, even when it honoured that authority just before.
///
/// Each round grants an admin, applies a grant under that admin, then runs
/// a second grant under the admin and the revoke of the admin together by
/// `runner`. The clause accepts either order for the pair: the racing grant
/// may answer `Ok` or `StaleAuthority`, and the revoke must apply. After
/// the pair, a grant under the admin must answer `StaleAuthority` and write
/// nothing. That sequential grant is the check that fails: it catches a
/// store that remembers an admin it once saw, or otherwise honours the
/// admin after the revoke. The clause runs several rounds, each with an
/// admin of its own.
///
/// The clause cannot observe how the pair interleaved inside the store. A
/// store that checks the admin's grant in one step and writes in another
/// passes it whenever its end state matches the order in which the grant
/// came first, so such a store needs a test of its own that the check and
/// the write are one atomic operation.
///
/// # Errors
///
/// Returns [`Clause::AnAdminsGrantNeverLandsAfterTheirRevoke`] otherwise.
pub async fn an_admins_grant_never_lands_after_their_revoke<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    let clause = Clause::AnAdminsGrantNeverLandsAfterTheirRevoke;
    let id = share(23, 0);
    let owner = Revoker::Owner(contract_fixture.owner.clone());
    for round in 0..ROUNDS {
        let base = usize::try_from(round).unwrap_or_default() * 4;
        let admin = super::person(base)?;
        grant_new(store, contract_fixture, &id, &admin, Role::Admin, 10).await?;
        let by = Granter::Admin(admin.clone());
        let before = super::person(base + 1)?;
        let warm = store
            .grant(write(&id, &before, Role::Reader, None, by.clone(), 20))
            .await;
        ensure(warm.is_ok(), clause, || {
            format!("round {round}: a grant under a current admin answered {warm:?}")
        })?;
        let racing = super::person(base + 2)?;
        let answers = runner
            .run_together(
                || store.grant(write(&id, &racing, Role::Reader, None, by.clone(), 30)),
                || store.revoke(revoke(&id, &admin, Role::Admin, owner.clone())),
            )
            .await;
        let (granted, revoked) = unexpected(answers.ok_or("a concurrent call panicked"))?;
        let after = super::person(base + 3)?;
        let late = store
            .grant(write(&id, &after, Role::Reader, None, by.clone(), 40))
            .await;
        let late_now = unexpected(store.get(&id, &after).await)?;
        ensure(
            revoked.is_ok()
                && matches!(granted, Ok(_) | Err(GrantStoreError::StaleAuthority))
                && late == Err(GrantStoreError::StaleAuthority)
                && late_now.is_none(),
            clause,
            || {
                format!(
                    "round {round}: the racing grant answered {granted:?}, the revoke {revoked:?}, \
                     a later grant {late:?}"
                )
            },
        )?;
    }
    Ok(())
}

/// A revoke or a grant under a revoked admin whose expected role is also
/// stale answers `StaleAuthority`, which comes before `Changed`, and keeps
/// the grant.
///
/// # Errors
///
/// Returns [`Clause::StaleAuthorityComesBeforeChanged`] otherwise.
pub async fn stale_authority_comes_before_changed<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let id = share(24, 0);
    let (admin, carol) = (&contract_fixture.admin, &contract_fixture.carol);
    grant_new(store, contract_fixture, &id, admin, Role::Admin, 10).await?;
    let kept = grant_new(store, contract_fixture, &id, carol, Role::Reader, 10).await?;
    let owner = Revoker::Owner(contract_fixture.owner.clone());
    unexpected(store.revoke(revoke(&id, admin, Role::Admin, owner)).await)?;
    let revoked = store
        .revoke(revoke(
            &id,
            carol,
            Role::Admin,
            Revoker::Admin(admin.clone()),
        ))
        .await;
    let granted = store
        .grant(write(
            &id,
            carol,
            Role::Reader,
            None,
            Granter::Admin(admin.clone()),
            20,
        ))
        .await;
    let carol_now = unexpected(store.get(&id, carol).await)?;
    ensure(
        revoked == Err(GrantStoreError::StaleAuthority)
            && granted == Err(GrantStoreError::StaleAuthority)
            && carol_now == Some(kept),
        Clause::StaleAuthorityComesBeforeChanged,
        || format!("the revoke answered {revoked:?}, the grant {granted:?}"),
    )
}

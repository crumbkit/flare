//! The grant store clauses about the limit on grants per share.

use super::{GrantFixture, person, revoke, share, write};
use crate::contract::{Clause, RunTogether, Violation, ensure, unexpected};
use crate::{GrantStore, GrantStoreError, MAX_GRANTS, Revoker, Role, ShareId};

/// Grants a Reader role to the people numbered `0..count` on `id`.
async fn fill<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
    id: &ShareId,
    count: usize,
) -> Result<(), Violation> {
    for number in 0..count {
        let grantee = person(number)?;
        let at = u64::try_from(number).unwrap_or_default();
        unexpected(
            store
                .grant(write(
                    id,
                    &grantee,
                    Role::Reader,
                    None,
                    contract_fixture.by_owner(),
                    at,
                ))
                .await,
        )?;
    }
    Ok(())
}

/// With [`MAX_GRANTS`] grants on a share, a grant to a new person answers
/// `TooMany` and writes nothing, a grantee can still change role, and after
/// a revoke a new person can be granted.
///
/// # Errors
///
/// Returns [`Clause::TheGrantLimitHolds`] otherwise.
pub async fn the_grant_limit_holds<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let clause = Clause::TheGrantLimitHolds;
    let id = share(30, 0);
    fill(store, contract_fixture, &id, MAX_GRANTS).await?;
    let by = contract_fixture.by_owner();
    let newcomer = &contract_fixture.carol;
    let refused = store
        .grant(write(&id, newcomer, Role::Reader, None, by.clone(), 1_000))
        .await;
    let newcomer_now = unexpected(store.get(&id, newcomer).await)?;
    ensure(
        refused == Err(GrantStoreError::TooMany) && newcomer_now.is_none(),
        clause,
        || format!("a grant past the limit answered {refused:?}"),
    )?;
    let first = person(0)?;
    let changed = store
        .grant(write(
            &id,
            &first,
            Role::Admin,
            Some(Role::Reader),
            by.clone(),
            1_001,
        ))
        .await;
    ensure(changed.is_ok(), clause, || {
        format!("a role change at the limit answered {changed:?}")
    })?;
    let owner = Revoker::Owner(contract_fixture.owner.clone());
    unexpected(
        store
            .revoke(revoke(&id, &person(1)?, Role::Reader, owner))
            .await,
    )?;
    let admitted = store
        .grant(write(&id, newcomer, Role::Reader, None, by, 1_002))
        .await;
    let count = unexpected(store.of_share(&id).await)?.len();
    ensure(admitted.is_ok() && count == MAX_GRANTS, clause, || {
        format!("a grant after a revoke answered {admitted:?}, with {count} grants")
    })
}

/// Two grants to new people on a share with one grant fewer than
/// [`MAX_GRANTS`], run together by `runner`, give one success and one
/// `TooMany`.
///
/// # Errors
///
/// Returns [`Clause::ConcurrentGrantsAtTheLimitGiveOneTooMany`] otherwise.
pub async fn concurrent_grants_at_the_limit_give_one_too_many<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    let id = share(31, 0);
    fill(store, contract_fixture, &id, MAX_GRANTS.saturating_sub(1)).await?;
    let by = contract_fixture.by_owner();
    let (bob, carol) = (&contract_fixture.bob, &contract_fixture.carol);
    let answers = runner
        .run_together(
            || store.grant(write(&id, bob, Role::Reader, None, by.clone(), 1_000)),
            || store.grant(write(&id, carol, Role::Reader, None, by.clone(), 1_000)),
        )
        .await;
    let (one, two) = unexpected(answers.ok_or("a concurrent grant panicked"))?;
    let count = unexpected(store.of_share(&id).await)?.len();
    let one_each = matches!(
        (&one, &two),
        (Ok(_), Err(GrantStoreError::TooMany)) | (Err(GrantStoreError::TooMany), Ok(_))
    );
    ensure(
        one_each && count == MAX_GRANTS,
        Clause::ConcurrentGrantsAtTheLimitGiveOneTooMany,
        || format!("two grants at the edge answered {one:?} and {two:?}, with {count} grants"),
    )
}

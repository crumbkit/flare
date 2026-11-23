//! The grant store clauses about grants and revokes that run together.

use super::{GrantFixture, grant_new, person, revoke, share, write};
use crate::contract::{Clause, RunTogether, Violation, ensure, unexpected};
use crate::{GrantStore, GrantStoreError, Revoker, Role};

/// How many pairs of calls each clause runs.
const ROUNDS: usize = 8;

/// Two grants to one person from no grant, run together by `runner`, give
/// one success and one `Changed` that names the winner's role, and the
/// store holds the winner's grant. The clause runs several pairs, each for
/// a person of its own.
///
/// # Errors
///
/// Returns [`Clause::ConcurrentGrantsFromNoGrantGiveOneChanged`] otherwise.
pub async fn concurrent_grants_from_no_grant_give_one_changed<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    let id = share(50, 0);
    let by = contract_fixture.by_owner();
    for round in 0..ROUNDS {
        let grantee = person(round)?;
        let answers = runner
            .run_together(
                || store.grant(write(&id, &grantee, Role::Reader, None, by.clone(), 10)),
                || store.grant(write(&id, &grantee, Role::Admin, None, by.clone(), 10)),
            )
            .await;
        let (one, two) = unexpected(answers.ok_or("a concurrent grant panicked"))?;
        let now = unexpected(store.get(&id, &grantee).await)?.map(|grant| grant.role());
        let winner = match (&one, &two) {
            (Ok(_), Err(GrantStoreError::Changed { current })) => {
                (*current == Some(Role::Reader)).then_some(Role::Reader)
            }
            (Err(GrantStoreError::Changed { current }), Ok(_)) => {
                (*current == Some(Role::Admin)).then_some(Role::Admin)
            }
            _ => None,
        };
        ensure(
            winner.is_some() && now == winner,
            Clause::ConcurrentGrantsFromNoGrantGiveOneChanged,
            || format!("round {round}: answers {one:?} and {two:?}, role now {now:?}"),
        )?;
    }
    Ok(())
}

/// A promotion of a Reader to Admin and a revoke of that Reader, run
/// together by `runner`, leave one of two consistent states: the promotion
/// won and the revoke answers `Changed` with Admin, or the revoke won and
/// the promotion answers `Changed` with no grant. Both lists agree with
/// `get`.
///
/// # Errors
///
/// Returns [`Clause::AGrantAndARevokeTogetherLeaveAConsistentState`]
/// otherwise.
pub async fn a_grant_and_a_revoke_together_leave_a_consistent_state<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    let clause = Clause::AGrantAndARevokeTogetherLeaveAConsistentState;
    let id = share(51, 0);
    let by = contract_fixture.by_owner();
    let owner = Revoker::Owner(contract_fixture.owner.clone());
    for round in 0..ROUNDS {
        let grantee = person(round)?;
        grant_new(store, contract_fixture, &id, &grantee, Role::Reader, 10).await?;
        let answers = runner
            .run_together(
                || {
                    store.grant(write(
                        &id,
                        &grantee,
                        Role::Admin,
                        Some(Role::Reader),
                        by.clone(),
                        20,
                    ))
                },
                || store.revoke(revoke(&id, &grantee, Role::Reader, owner.clone())),
            )
            .await;
        let (granted, revoked) = unexpected(answers.ok_or("a concurrent call panicked"))?;
        let now = unexpected(store.get(&id, &grantee).await)?.map(|grant| grant.role());
        let in_share = unexpected(store.of_share(&id).await)?
            .iter()
            .find(|grant| grant.grantee() == &grantee)
            .map(|grant| grant.role());
        let in_person = unexpected(store.granted_to(&grantee).await)?
            .items()
            .iter()
            .find(|grant| grant.share() == &id)
            .map(|grant| grant.role());
        let promoted = granted.is_ok()
            && revoked
                == Err(GrantStoreError::Changed {
                    current: Some(Role::Admin),
                })
            && now == Some(Role::Admin);
        let removed = granted == Err(GrantStoreError::Changed { current: None })
            && revoked.is_ok()
            && now.is_none();
        ensure(
            (promoted || removed) && in_share == now && in_person == now,
            clause,
            || format!("round {round}: grant {granted:?}, revoke {revoked:?}, role {now:?}"),
        )?;
    }
    Ok(())
}

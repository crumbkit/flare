//! The share store clauses about calls that run together.

use super::{ShareFixture, numbered_id};
use crate::contract::{Clause, RunTogether, Violation, ensure, unexpected};
use crate::{NewShare, ShareStore, ShareStoreError, Timestamp};

/// How many pairs of calls each clause runs.
const ROUNDS: u32 = 8;

/// Two creates of one id that run together give one success and one
/// `IdTaken`, and the share is the winner's. The clause runs several pairs,
/// each with an id of its own, and `runner` drives each pair.
///
/// # Errors
///
/// Returns [`Clause::ConcurrentCreatesOfOneIdGiveOneIdTaken`] otherwise.
pub async fn concurrent_creates_of_one_id_give_one_id_taken<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    for round in 0..ROUNDS {
        let id = numbered_id(20, round);
        let first = contract_fixture.new_share(&id, &contract_fixture.owner, 10);
        let second = NewShare::new(
            id.clone(),
            contract_fixture.other_owner.clone(),
            contract_fixture.other_title.clone(),
            contract_fixture.content(1).clone(),
            Timestamp::from_secs(20),
        );
        let answers = runner
            .run_together(
                || store.create(first.clone()),
                || store.create(second.clone()),
            )
            .await;
        let (one, two) = unexpected(answers.ok_or("a concurrent create panicked"))?;
        let share = unexpected(store.get(&id).await)?;
        let winner = match (&one, &two) {
            (Ok(_), Err(ShareStoreError::IdTaken)) => Some(first.share()),
            (Err(ShareStoreError::IdTaken), Ok(_)) => Some(second.share()),
            _ => None,
        };
        ensure(
            winner.is_some_and(|winner| winner == share),
            Clause::ConcurrentCreatesOfOneIdGiveOneIdTaken,
            || format!("round {round}: answers {one:?} and {two:?}, share {share:?}"),
        )?;
    }
    Ok(())
}

/// Two publishes that run together give two consecutive versions, each of
/// which reads back with its own content. The clause runs several pairs on
/// one share, and `runner` drives each pair.
///
/// # Errors
///
/// Returns [`Clause::ConcurrentPublishesGetConsecutiveVersions`] otherwise.
pub async fn concurrent_publishes_get_consecutive_versions<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    let clause = Clause::ConcurrentPublishesGetConsecutiveVersions;
    let id = numbered_id(21, 0);
    unexpected(
        store
            .create(contract_fixture.new_share(&id, &contract_fixture.owner, 10))
            .await,
    )?;
    for round in 0..ROUNDS {
        let before = unexpected(store.get(&id).await)?.latest().get();
        let first = contract_fixture.version(1, 20);
        let second = contract_fixture.version(2, 30);
        let answers = runner
            .run_together(
                || store.publish(&id, first.clone()),
                || store.publish(&id, second.clone()),
            )
            .await;
        let (one, two) = unexpected(answers.ok_or("a concurrent publish panicked"))?;
        let (Ok(one), Ok(two)) = (one, two) else {
            return Err(Violation::new(
                clause,
                format!("round {round}: publishes answered {one:?} and {two:?}"),
            ));
        };
        let mut numbers = [one.get(), two.get()];
        numbers.sort_unstable();
        let latest = unexpected(store.get(&id).await)?.latest().get();
        let expected = [before.saturating_add(1), before.saturating_add(2)];
        ensure(numbers == expected && latest == expected[1], clause, || {
            format!("round {round}: from {before} the publishes gave {numbers:?}, latest {latest}")
        })?;
        let read_one = store.version(&id, one).await;
        let read_two = store.version(&id, two).await;
        ensure(
            read_one == Ok(first.shared(id.clone(), one))
                && read_two == Ok(second.shared(id.clone(), two)),
            clause,
            || format!("round {round}: the versions read {read_one:?} and {read_two:?}"),
        )?;
    }
    Ok(())
}

//! The share store clauses about the owner's list.

use super::{ShareFixture, numbered_id};
use crate::contract::{Clause, Violation, ensure, unexpected};
use crate::{ShareId, ShareStore};

/// The owner's list holds exactly that owner's shares, newest first by
/// creation time and then by id.
///
/// # Errors
///
/// Returns [`Clause::TheOwnersListHoldsOnlyTheirSharesNewestFirst`]
/// otherwise.
pub async fn the_owners_list_holds_only_their_shares_newest_first<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let owner = &contract_fixture.owner;
    // Created out of order; the two at 30 tie and are ordered by id.
    let plan = [(1, 10), (2, 30), (3, 20), (0, 30)];
    for (number, at) in plan {
        unexpected(
            store
                .create(contract_fixture.new_share(&numbered_id(9, number), owner, at))
                .await,
        )?;
    }
    let others = numbered_id(9, 9);
    unexpected(
        store
            .create(contract_fixture.new_share(&others, &contract_fixture.other_owner, 40))
            .await,
    )?;
    let listed = unexpected(store.owned_by(owner).await)?;
    let ids: Vec<ShareId> = listed
        .items()
        .iter()
        .map(|share| share.id().clone())
        .collect();
    let expected: Vec<ShareId> = [0, 2, 3, 1].map(|number| numbered_id(9, number)).to_vec();
    ensure(
        ids == expected
            && !listed.truncated()
            && listed.items().iter().all(|share| share.owner() == owner),
        Clause::TheOwnersListHoldsOnlyTheirSharesNewestFirst,
        || format!("the list is {:?}", listed.items()),
    )
}

/// A share is in its owner's list as soon as it is created.
///
/// # Errors
///
/// Returns [`Clause::AShareIsListedOnceCreated`] otherwise.
pub async fn a_share_is_listed_once_created<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let owner = &contract_fixture.owner;
    for count in 1..=3_u32 {
        let id = numbered_id(10, count);
        unexpected(
            store
                .create(contract_fixture.new_share(&id, owner, u64::from(count)))
                .await,
        )?;
        let listed = unexpected(store.owned_by(owner).await)?;
        ensure(
            listed.items().iter().any(|share| share.id() == &id)
                && listed.items().len() == usize::try_from(count).unwrap_or_default(),
            Clause::AShareIsListedOnceCreated,
            || format!("after {count} creates the list is {:?}", listed.items()),
        )?;
    }
    Ok(())
}

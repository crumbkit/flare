//! The grant store clauses about the share list and the person list.

use super::{GrantFixture, grant_new, person, share};
use crate::contract::{Clause, Violation, ensure, unexpected};
use crate::{GrantStore, MAX_LISTED, PrincipalId, Role, ShareId};

/// A share's list holds only that share's grants, and a grant on one share
/// is not visible on another, through `get` or the list.
///
/// # Errors
///
/// Returns [`Clause::AShareListHoldsOnlyItsGrants`] otherwise.
pub async fn a_share_list_holds_only_its_grants<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let (first, second) = (share(40, 0), share(40, 1));
    let (bob, carol) = (&contract_fixture.bob, &contract_fixture.carol);
    let bobs = grant_new(store, contract_fixture, &first, bob, Role::Reader, 10).await?;
    let carols = grant_new(store, contract_fixture, &second, carol, Role::Admin, 10).await?;
    let first_list = unexpected(store.of_share(&first).await)?;
    let second_list = unexpected(store.of_share(&second).await)?;
    let crossed = unexpected(store.get(&second, bob).await)?;
    ensure(
        first_list == [bobs] && second_list == [carols] && crossed.is_none(),
        Clause::AShareListHoldsOnlyItsGrants,
        || format!("lists {first_list:?} and {second_list:?}; bob on the second {crossed:?}"),
    )
}

/// A person's list holds that person's grants across shares, newest first,
/// and says it is not truncated.
///
/// # Errors
///
/// Returns [`Clause::APersonsListHoldsTheirGrantsNewestFirst`] otherwise.
pub async fn a_persons_list_holds_their_grants_newest_first<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let (bob, carol) = (&contract_fixture.bob, &contract_fixture.carol);
    for (number, at) in [(0, 10), (1, 30), (2, 20)] {
        grant_new(
            store,
            contract_fixture,
            &share(41, number),
            bob,
            Role::Reader,
            at,
        )
        .await?;
    }
    grant_new(
        store,
        contract_fixture,
        &share(41, 3),
        carol,
        Role::Reader,
        40,
    )
    .await?;
    let listed = unexpected(store.granted_to(bob).await)?;
    let shares: Vec<&ShareId> = listed.items().iter().map(|grant| grant.share()).collect();
    let expected = [share(41, 1), share(41, 2), share(41, 0)];
    ensure(
        shares == expected.iter().collect::<Vec<_>>()
            && !listed.truncated()
            && listed.items().iter().all(|grant| grant.grantee() == bob),
        Clause::APersonsListHoldsTheirGrantsNewestFirst,
        || format!("bob's list is {:?}", listed.items()),
    )
}

/// A person with more grants than [`MAX_LISTED`] gets the newest
/// [`MAX_LISTED`] of them with the truncation flag set, ties ordered by
/// share, and a share's list is ordered by time of first grant and then by
/// grantee.
///
/// # Errors
///
/// Returns [`Clause::AListShowsItsTruncationAndOrder`] otherwise.
pub async fn a_list_shows_its_truncation_and_order<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    let clause = Clause::AListShowsItsTruncationAndOrder;
    let bob = &contract_fixture.bob;
    let count = MAX_LISTED + 1;
    for number in 0..count {
        let id = share(42, u32::try_from(number).unwrap_or_default());
        // Pairs of shares share a time, so ties are ordered by share.
        let at = u64::try_from(number / 2).unwrap_or_default();
        grant_new(store, contract_fixture, &id, bob, Role::Reader, at).await?;
    }
    let listed = unexpected(store.granted_to(bob).await)?;
    let ordered = listed.items().windows(2).all(|pair| match pair {
        [newer, older] => {
            newer.created() > older.created()
                || (newer.created() == older.created() && newer.share() < older.share())
        }
        _ => true,
    });
    let newest = listed.items().first().map(|grant| grant.share().clone());
    ensure(
        listed.truncated()
            && listed.items().len() == MAX_LISTED
            && ordered
            && newest == Some(share(42, u32::try_from(count - 1).unwrap_or_default())),
        clause,
        || {
            format!(
                "{} entries, truncated {}",
                listed.items().len(),
                listed.truncated()
            )
        },
    )?;
    let id = share(43, 0);
    let people: Vec<PrincipalId> = (0..4).map(person).collect::<Result<_, _>>()?;
    for (grantee, at) in people.iter().rev().zip([5, 5, 1, 9]) {
        grant_new(store, contract_fixture, &id, grantee, Role::Reader, at).await?;
    }
    let of_share = unexpected(store.of_share(&id).await)?;
    let order: Vec<&PrincipalId> = of_share.iter().map(|grant| grant.grantee()).collect();
    let [p0, p1, p2, p3] = people.as_slice() else {
        return Err(Violation::new(Clause::FixtureIsUnusable, "four people"));
    };
    ensure(order == [p1, p2, p3, p0], clause, || {
        format!("the share list is {of_share:?}")
    })
}

//! The [`ResponseStore`] contract suite. Every function writes the links it
//! needs, so the caller passes a new, empty store.

pub mod index;

use super::{Clause, RunTogether, Violation, ensure, unexpected};
use crate::{NewLink, Position, ResponseStore, ShareId, ShareVersion, Timestamp};

/// The flares and responses that the suite uses.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ResponseFixture {
    version: ShareVersion,
}

impl ResponseFixture {
    /// The standard fixture.
    ///
    /// # Errors
    ///
    /// Never; the signature matches the other suites' fixtures.
    pub fn standard() -> Result<Self, Violation> {
        Ok(Self {
            version: ShareVersion::first(),
        })
    }

    fn link(&self, response: u8, flare: u8, at: u64) -> NewLink {
        NewLink::new(
            id(response),
            id(flare),
            self.version,
            Timestamp::from_secs(at),
        )
    }
}

fn id(tag: u8) -> ShareId {
    ShareId::from_random([tag; 16])
}

/// Every position: no upper bound on a flare's list.
const ALL: Position = Position::new(u64::MAX);

/// A link reads back under its flare with its response, version and time.
///
/// # Errors
///
/// Returns [`Clause::ALinkReadsBackUnderItsFlare`] otherwise.
pub async fn a_link_reads_back_under_its_flare<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    let link = contract_fixture.link(2, 1, 30);
    let stored = unexpected(store.link(link.clone()).await)?;
    let listed = unexpected(store.to_flare(&id(1), Position::new(0), ALL, 10).await)?;
    ensure(
        listed == vec![stored.clone()] && stored == link.at_position(stored.position),
        Clause::ALinkReadsBackUnderItsFlare,
        || format!("stored {stored:?}, listed {listed:?}"),
    )
}

/// Each new link gets a larger position, and the head is the last one
/// given.
///
/// # Errors
///
/// Returns [`Clause::PositionsIncreaseAndTheHeadFollows`] otherwise.
pub async fn positions_increase_and_the_head_follows<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    let empty = unexpected(store.head().await)?;
    let first = unexpected(store.link(contract_fixture.link(2, 1, 30)).await)?;
    let second = unexpected(store.link(contract_fixture.link(3, 4, 31)).await)?;
    let head = unexpected(store.head().await)?;
    ensure(
        empty == Position::new(0)
            && first.position > empty
            && second.position > first.position
            && head == second.position,
        Clause::PositionsIncreaseAndTheHeadFollows,
        || format!("empty head {empty:?}, first {first:?}, second {second:?}, head {head:?}"),
    )
}

/// A repeated link of one response and flare answers the first link and
/// adds nothing.
///
/// # Errors
///
/// Returns [`Clause::ARepeatedLinkKeepsItsFirstPosition`] otherwise.
pub async fn a_repeated_link_keeps_its_first_position<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    let first = unexpected(store.link(contract_fixture.link(2, 1, 30)).await)?;
    let again = unexpected(store.link(contract_fixture.link(2, 1, 40)).await)?;
    let listed = unexpected(store.to_flare(&id(1), Position::new(0), ALL, 10).await)?;
    ensure(
        again == first && listed.len() == 1,
        Clause::ARepeatedLinkKeepsItsFirstPosition,
        || format!("first {first:?}, again {again:?}, listed {listed:?}"),
    )
}

/// Two links that run together get two different positions, both at or
/// below the head.
///
/// # Errors
///
/// Returns [`Clause::ConcurrentLinksGetDistinctPositions`] otherwise.
pub async fn concurrent_links_get_distinct_positions<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    for round in 0..8_u8 {
        let one = contract_fixture.link(round.saturating_add(10), 1, 30);
        let two = contract_fixture.link(round.saturating_add(40), 1, 30);
        let answers = runner
            .run_together(|| store.link(one.clone()), || store.link(two.clone()))
            .await;
        let (first, second) = unexpected(answers.ok_or("a concurrent link panicked"))?;
        let (first, second) = (unexpected(first)?, unexpected(second)?);
        let head = unexpected(store.head().await)?;
        ensure(
            first.position != second.position && first.position <= head && second.position <= head,
            Clause::ConcurrentLinksGetDistinctPositions,
            || format!("round {round}: {first:?} and {second:?}, head {head:?}"),
        )?;
    }
    Ok(())
}

/// A flare's list holds the links above the given position and at most the
/// bound.
///
/// # Errors
///
/// Returns [`Clause::AFlaresListHoldsOnlyNewerLinksUpToTheBound`] otherwise.
pub async fn a_flares_list_holds_only_newer_links_up_to_the_bound<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    let mut stored = Vec::new();
    for response in 2..=5 {
        stored.push(unexpected(
            store.link(contract_fixture.link(response, 1, 30)).await,
        )?);
    }
    let positions: Vec<Position> = stored.iter().map(|link| link.position).collect();
    let (Some(first), Some(third)) = (positions.first(), positions.get(2)) else {
        return Err(Violation::new(
            Clause::AFlaresListHoldsOnlyNewerLinksUpToTheBound,
            "no links",
        ));
    };
    let listed = unexpected(store.to_flare(&id(1), *first, *third, 10).await)?;
    let got: Vec<Position> = listed.iter().map(|link| link.position).collect();
    ensure(
        got == positions.get(1..3).unwrap_or_default(),
        Clause::AFlaresListHoldsOnlyNewerLinksUpToTheBound,
        || format!("positions {positions:?}, listed after the first up to the third {got:?}"),
    )
}

/// A flare's list holds only links to that flare.
///
/// # Errors
///
/// Returns [`Clause::AFlaresListHoldsOnlyItsLinks`] otherwise.
pub async fn a_flares_list_holds_only_its_links<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    unexpected(store.link(contract_fixture.link(2, 1, 30)).await)?;
    unexpected(store.link(contract_fixture.link(3, 4, 30)).await)?;
    let listed = unexpected(store.to_flare(&id(4), Position::new(0), ALL, 10).await)?;
    ensure(
        listed.len() == 1 && listed.iter().all(|link| link.flare == id(4)),
        Clause::AFlaresListHoldsOnlyItsLinks,
        || format!("{listed:?}"),
    )
}

/// A response's list names every flare it links to.
///
/// # Errors
///
/// Returns [`Clause::AResponsesListNamesEveryFlare`] otherwise.
pub async fn a_responses_list_names_every_flare<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    unexpected(store.link(contract_fixture.link(2, 1, 30)).await)?;
    unexpected(store.link(contract_fixture.link(2, 4, 30)).await)?;
    unexpected(store.link(contract_fixture.link(3, 1, 30)).await)?;
    let listed = unexpected(store.of_response(&id(2)).await)?;
    let mut flares: Vec<ShareId> = listed.iter().map(|link| link.flare.clone()).collect();
    flares.sort();
    ensure(
        flares == vec![id(1), id(4)],
        Clause::AResponsesListNamesEveryFlare,
        || format!("{listed:?}"),
    )
}

/// A flare's list holds at most the limit, oldest first.
///
/// # Errors
///
/// Returns [`Clause::TheLimitBoundsAListOldestFirst`] otherwise.
pub async fn the_limit_bounds_a_list_oldest_first<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    for response in 2..=6 {
        unexpected(store.link(contract_fixture.link(response, 1, 30)).await)?;
    }
    let listed = unexpected(store.to_flare(&id(1), Position::new(0), ALL, 3).await)?;
    let responses: Vec<ShareId> = listed.iter().map(|link| link.response.clone()).collect();
    ensure(
        responses == vec![id(2), id(3), id(4)],
        Clause::TheLimitBoundsAListOldestFirst,
        || format!("{listed:?}"),
    )
}

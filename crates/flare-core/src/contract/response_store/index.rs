//! The clauses of the [`ResponseStore`] activity index: `index`, `unindex`,
//! `visible` and `pairs`. Every function writes the links and entries it
//! needs, so the caller passes a new, empty store.

use super::{ALL, ResponseFixture, id};
use crate::contract::{Clause, RunTogether, Violation, ensure, unexpected};
use crate::{
    IndexEntry, NewEntry, Position, PrincipalId, Reader, ResponseLink, ResponseStore, ShareId,
};

/// The reader `name`@example.com.
fn reader(name: &str) -> Result<Reader, Violation> {
    PrincipalId::try_from(format!("{name}@example.com").as_str())
        .map(Reader::Person)
        .map_err(|error| Violation::new(Clause::FixtureIsUnusable, error))
}

/// Writes the link from `response` to `flare` and answers it.
async fn linked<R: ResponseStore>(
    store: &R,
    fixture: &ResponseFixture,
    response: u8,
    flare: u8,
) -> Result<ResponseLink, Violation> {
    unexpected(store.link(fixture.link(response, flare, 30)).await)
}

/// Every entry of `reader` on `flare`.
async fn listed<R: ResponseStore>(
    store: &R,
    reader: &Reader,
    flare: u8,
) -> Result<Vec<IndexEntry>, Violation> {
    unexpected(
        store
            .visible(reader, &id(flare), Position::new(0), ALL, 1_000)
            .await,
    )
}

/// The responses of `entries`, in order.
fn responses(entries: &[IndexEntry]) -> Vec<ShareId> {
    entries.iter().map(|entry| entry.response.clone()).collect()
}

/// An index entry reads back for its reader on its flare, with the link's
/// response, flare, version and time, and for no other reader or flare.
///
/// # Errors
///
/// Returns [`Clause::AnEntryReadsBackOnlyForItsReaderAndFlare`] otherwise.
pub async fn an_entry_reads_back_only_for_its_reader_and_flare<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    let (ana, bo) = (reader("ana")?, reader("bo")?);
    let link = linked(store, contract_fixture, 2, 1).await?;
    unexpected(
        store
            .index(vec![NewEntry::new(ana.clone(), link.clone())])
            .await,
    )?;
    let own = listed(store, &ana, 1).await?;
    let other_reader = listed(store, &bo, 1).await?;
    let other_flare = listed(store, &ana, 4).await?;
    let read_back = match own.as_slice() {
        [entry] => *entry == NewEntry::new(ana.clone(), link.clone()).at_position(entry.position),
        _ => false,
    };
    ensure(
        read_back && other_reader.is_empty() && other_flare.is_empty(),
        Clause::AnEntryReadsBackOnlyForItsReaderAndFlare,
        || format!("own {own:?}, other reader {other_reader:?}, other flare {other_flare:?}"),
    )
}

/// Each new entry takes a position above every earlier link and entry, and
/// the head is the last one given.
///
/// # Errors
///
/// Returns [`Clause::EachEntryTakesANewPositionAndTheHeadFollows`]
/// otherwise.
pub async fn each_entry_takes_a_new_position_and_the_head_follows<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    let link = linked(store, contract_fixture, 2, 1).await?;
    let entries = vec![
        NewEntry::new(reader("ana")?, link.clone()),
        NewEntry::new(reader("bo")?, link.clone()),
        NewEntry::new(reader("cy")?, link.clone()),
    ];
    unexpected(store.index(entries).await)?;
    let mut positions = Vec::new();
    for name in ["ana", "bo", "cy"] {
        positions.extend(
            listed(store, &reader(name)?, 1)
                .await?
                .iter()
                .map(|entry| entry.position),
        );
    }
    let head = unexpected(store.head().await)?;
    let later = linked(store, contract_fixture, 3, 1).await?;
    let mut sorted = positions.clone();
    sorted.sort();
    sorted.dedup();
    ensure(
        sorted.len() == 3
            && sorted.iter().all(|position| *position > link.position)
            && sorted.last() == Some(&head)
            && later.position > head,
        Clause::EachEntryTakesANewPositionAndTheHeadFollows,
        || format!("link {link:?}, entries {positions:?}, head {head:?}, later link {later:?}"),
    )
}

/// A repeated entry of one reader, response and flare keeps its first
/// position and moves no position.
///
/// # Errors
///
/// Returns [`Clause::ARepeatedEntryKeepsItsFirstPosition`] otherwise.
pub async fn a_repeated_entry_keeps_its_first_position<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    let ana = reader("ana")?;
    let link = linked(store, contract_fixture, 2, 1).await?;
    unexpected(
        store
            .index(vec![NewEntry::new(ana.clone(), link.clone())])
            .await,
    )?;
    let first = listed(store, &ana, 1).await?;
    let head = unexpected(store.head().await)?;
    unexpected(
        store
            .index(vec![
                NewEntry::new(ana.clone(), link.clone()),
                NewEntry::new(ana.clone(), link),
            ])
            .await,
    )?;
    let again = listed(store, &ana, 1).await?;
    let head_again = unexpected(store.head().await)?;
    ensure(
        first.len() == 1 && again == first && head_again == head,
        Clause::ARepeatedEntryKeepsItsFirstPosition,
        || format!("first {first:?} at head {head:?}, again {again:?} at head {head_again:?}"),
    )
}

/// Unindexing a reader from a response removes that reader's entries for
/// it on every flare, and leaves other responses and other readers.
///
/// # Errors
///
/// Returns [`Clause::UnindexRemovesOnlyTheReadersEntriesForTheResponse`]
/// otherwise.
pub async fn unindex_removes_only_the_readers_entries_for_the_response<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    let (ana, bo) = (reader("ana")?, reader("bo")?);
    let to_one = linked(store, contract_fixture, 2, 1).await?;
    let to_four = linked(store, contract_fixture, 2, 4).await?;
    let other = linked(store, contract_fixture, 3, 1).await?;
    unexpected(
        store
            .index(vec![
                NewEntry::new(ana.clone(), to_one.clone()),
                NewEntry::new(ana.clone(), to_four),
                NewEntry::new(ana.clone(), other),
                NewEntry::new(bo.clone(), to_one.clone()),
            ])
            .await,
    )?;
    unexpected(store.unindex(&ana, &id(2)).await)?;
    unexpected(store.unindex(&ana, &id(2)).await)?;
    let ana_one = responses(&listed(store, &ana, 1).await?);
    let ana_four = responses(&listed(store, &ana, 4).await?);
    let bo_one = responses(&listed(store, &bo, 1).await?);
    let removed = ana_one == vec![id(3)] && ana_four.is_empty() && bo_one == vec![id(2)];
    // A new entry after the removal takes a new position.
    let head = unexpected(store.head().await)?;
    unexpected(store.index(vec![NewEntry::new(ana.clone(), to_one)]).await)?;
    let back = listed(store, &ana, 1).await?;
    let renewed = back
        .iter()
        .any(|entry| entry.response == id(2) && entry.position > head);
    ensure(
        removed && renewed,
        Clause::UnindexRemovesOnlyTheReadersEntriesForTheResponse,
        || {
            format!(
                "ana on 1 {ana_one:?}, ana on 4 {ana_four:?}, bo on 1 {bo_one:?}, back {back:?}"
            )
        },
    )
}

/// A reader's list on a flare holds the entries above the given position
/// and at most the bound, oldest first, at most the limit.
///
/// # Errors
///
/// Returns [`Clause::AReadersListHoldsOnlyNewerEntriesUpToTheBound`]
/// otherwise.
pub async fn a_readers_list_holds_only_newer_entries_up_to_the_bound<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    let ana = reader("ana")?;
    for response in 2..=6 {
        let link = linked(store, contract_fixture, response, 1).await?;
        unexpected(store.index(vec![NewEntry::new(ana.clone(), link)]).await)?;
    }
    let all = listed(store, &ana, 1).await?;
    let positions: Vec<Position> = all.iter().map(|entry| entry.position).collect();
    let (Some(first), Some(fourth)) = (positions.first(), positions.get(3)) else {
        return Err(Violation::new(
            Clause::AReadersListHoldsOnlyNewerEntriesUpToTheBound,
            format!("five entries listed as {all:?}"),
        ));
    };
    let bounded = unexpected(store.visible(&ana, &id(1), *first, *fourth, 10).await)?;
    let limited = unexpected(store.visible(&ana, &id(1), *first, ALL, 2).await)?;
    let bounded: Vec<Position> = bounded.iter().map(|entry| entry.position).collect();
    let limited: Vec<Position> = limited.iter().map(|entry| entry.position).collect();
    ensure(
        responses(&all) == (2..=6).map(id).collect::<Vec<_>>()
            && bounded == positions.get(1..4).unwrap_or_default()
            && limited == positions.get(1..3).unwrap_or_default(),
        Clause::AReadersListHoldsOnlyNewerEntriesUpToTheBound,
        || format!("all {positions:?}, bounded {bounded:?}, limited {limited:?}"),
    )
}

/// One index call of 120 entries writes each of them with its own
/// position, so a store that writes in batches writes every batch.
///
/// # Errors
///
/// Returns [`Clause::AnIndexOfManyEntriesWritesEach`] otherwise.
pub async fn an_index_of_many_entries_writes_each<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    let link = linked(store, contract_fixture, 2, 1).await?;
    let mut readers = Vec::new();
    for n in 0..120 {
        readers.push(reader(&format!("p{n}"))?);
    }
    let entries = readers
        .iter()
        .map(|person| NewEntry::new(person.clone(), link.clone()))
        .collect();
    unexpected(store.index(entries).await)?;
    let mut positions = Vec::new();
    for person in &readers {
        positions.extend(
            listed(store, person, 1)
                .await?
                .iter()
                .map(|entry| entry.position),
        );
    }
    let head = unexpected(store.head().await)?;
    let count = positions.len();
    positions.sort();
    positions.dedup();
    ensure(
        count == 120 && positions.len() == 120 && positions.last() == Some(&head),
        Clause::AnIndexOfManyEntriesWritesEach,
        || {
            format!(
                "{count} entries listed, {} positions, head {head:?}",
                positions.len()
            )
        },
    )
}

/// `pairs` answers the link from each named response to the flare, in the
/// order named, and nothing for a response without one.
///
/// # Errors
///
/// Returns [`Clause::PairsNameTheLinksFromEachResponseToTheFlare`]
/// otherwise.
pub async fn pairs_name_the_links_from_each_response_to_the_flare<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
) -> Result<(), Violation> {
    let three = linked(store, contract_fixture, 3, 1).await?;
    let two = linked(store, contract_fixture, 2, 1).await?;
    linked(store, contract_fixture, 2, 4).await?;
    linked(store, contract_fixture, 5, 4).await?;
    let mut named: Vec<ShareId> = vec![id(2), id(5), id(3), id(6)];
    // More than one batch of a store that reads 100 pairs at a time.
    named.extend((100..=220).map(id));
    let pairs = unexpected(store.pairs(&named, &id(1)).await)?;
    ensure(
        pairs == vec![two.clone(), three.clone()],
        Clause::PairsNameTheLinksFromEachResponseToTheFlare,
        || format!("expected {two:?} and {three:?}, got {pairs:?}"),
    )
}

/// Two index calls that run together give their entries distinct
/// positions, all at or below the head.
///
/// # Errors
///
/// Returns [`Clause::ConcurrentIndexesGetDistinctPositions`] otherwise.
pub async fn concurrent_indexes_get_distinct_positions<R: ResponseStore>(
    store: &R,
    contract_fixture: &ResponseFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    let link = linked(store, contract_fixture, 2, 1).await?;
    for round in 0..8_u8 {
        let one = reader(&format!("one{round}"))?;
        let two = reader(&format!("two{round}"))?;
        let first = vec![NewEntry::new(one.clone(), link.clone())];
        let second = vec![NewEntry::new(two.clone(), link.clone())];
        let answers = runner
            .run_together(
                || store.index(first.clone()),
                || store.index(second.clone()),
            )
            .await;
        let (left, right) = unexpected(answers.ok_or("a concurrent index panicked"))?;
        unexpected(left)?;
        unexpected(right)?;
        let head = unexpected(store.head().await)?;
        let mut positions: Vec<Position> = listed(store, &one, 1)
            .await?
            .iter()
            .chain(listed(store, &two, 1).await?.iter())
            .map(|entry| entry.position)
            .collect();
        positions.dedup();
        ensure(
            positions.len() == 2 && positions.iter().all(|position| *position <= head),
            Clause::ConcurrentIndexesGetDistinctPositions,
            || format!("round {round}: positions {positions:?}, head {head:?}"),
        )?;
    }
    Ok(())
}

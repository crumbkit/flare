//! The [`CursorStore`] contract suite. Every function writes the cursors
//! it needs, so the caller passes a new, empty store.

use super::{Clause, Violation, ensure, fixture, unexpected};
use crate::{CommentStoreError, CursorName, CursorStore, Position, PrincipalId};

/// The people and names that the suite uses.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CursorFixture {
    ana: PrincipalId,
    jo: PrincipalId,
}

impl CursorFixture {
    /// The standard fixture.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by
    /// its type, which means this crate is broken.
    pub fn standard() -> Result<Self, Violation> {
        Ok(Self {
            ana: fixture(PrincipalId::try_from("ana@example.org"))?,
            jo: fixture(PrincipalId::try_from("jo@example.org"))?,
        })
    }
}

fn name(text: &str) -> Result<CursorName, Violation> {
    fixture(CursorName::try_from(text))
}

/// A cursor that was never advanced is absent, and an advance makes it
/// and reads back.
///
/// # Errors
///
/// Returns [`Clause::AnAdvanceMakesTheCursor`] otherwise.
pub async fn an_advance_makes_the_cursor<U: CursorStore>(
    store: &U,
    contract_fixture: &CursorFixture,
) -> Result<(), Violation> {
    let before = unexpected(store.get(&contract_fixture.ana, &name("inbox")?).await)?;
    let held = unexpected(
        store
            .advance(&contract_fixture.ana, &name("inbox")?, Position::new(7))
            .await,
    )?;
    let after = unexpected(store.get(&contract_fixture.ana, &name("inbox")?).await)?;
    ensure(
        before.is_none() && held == Position::new(7) && after == Some(Position::new(7)),
        Clause::AnAdvanceMakesTheCursor,
        || format!("{before:?}, {held:?}, {after:?}"),
    )
}

/// An advance to a lower position keeps the higher one and answers it.
///
/// # Errors
///
/// Returns [`Clause::ACursorNeverMovesBack`] otherwise.
pub async fn a_cursor_never_moves_back<U: CursorStore>(
    store: &U,
    contract_fixture: &CursorFixture,
) -> Result<(), Violation> {
    let cursor = name("inbox")?;
    unexpected(
        store
            .advance(&contract_fixture.ana, &cursor, Position::new(9))
            .await,
    )?;
    let lower = unexpected(
        store
            .advance(&contract_fixture.ana, &cursor, Position::new(4))
            .await,
    )?;
    let higher = unexpected(
        store
            .advance(&contract_fixture.ana, &cursor, Position::new(12))
            .await,
    )?;
    let held = unexpected(store.get(&contract_fixture.ana, &cursor).await)?;
    ensure(
        lower == Position::new(9) && higher == Position::new(12) && held == Some(Position::new(12)),
        Clause::ACursorNeverMovesBack,
        || format!("{lower:?}, {higher:?}, {held:?}"),
    )
}

/// Cursors of two people or two names do not meet.
///
/// # Errors
///
/// Returns [`Clause::CursorsAreKeptApart`] otherwise.
pub async fn cursors_are_kept_apart<U: CursorStore>(
    store: &U,
    contract_fixture: &CursorFixture,
) -> Result<(), Violation> {
    unexpected(
        store
            .advance(&contract_fixture.ana, &name("one")?, Position::new(3))
            .await,
    )?;
    unexpected(
        store
            .advance(&contract_fixture.jo, &name("one")?, Position::new(5))
            .await,
    )?;
    let ana_two = unexpected(store.get(&contract_fixture.ana, &name("two")?).await)?;
    let ana_one = unexpected(store.get(&contract_fixture.ana, &name("one")?).await)?;
    let jo_one = unexpected(store.get(&contract_fixture.jo, &name("one")?).await)?;
    ensure(
        ana_two.is_none() && ana_one == Some(Position::new(3)) && jo_one == Some(Position::new(5)),
        Clause::CursorsAreKeptApart,
        || format!("{ana_two:?}, {ana_one:?}, {jo_one:?}"),
    )
}

/// A person who holds `limit` cursors cannot make another and can still
/// move one they hold, and another person is not affected. The caller
/// builds the store with that limit.
///
/// # Errors
///
/// Returns [`Clause::TheCursorLimitHolds`] otherwise.
pub async fn the_cursor_limit_holds<U: CursorStore>(
    store: &U,
    contract_fixture: &CursorFixture,
    limit: u8,
) -> Result<(), Violation> {
    for number in 1..=limit {
        unexpected(
            store
                .advance(
                    &contract_fixture.ana,
                    &name(&format!("c{number}"))?,
                    Position::new(1),
                )
                .await,
        )?;
    }
    let over = store
        .advance(
            &contract_fixture.ana,
            &name("one-too-many")?,
            Position::new(1),
        )
        .await;
    let moved = unexpected(
        store
            .advance(&contract_fixture.ana, &name("c1")?, Position::new(8))
            .await,
    )?;
    let other = store
        .advance(
            &contract_fixture.jo,
            &name("one-too-many")?,
            Position::new(1),
        )
        .await;
    ensure(
        over == Err(CommentStoreError::TooMany) && moved == Position::new(8) && other.is_ok(),
        Clause::TheCursorLimitHolds,
        || format!("{over:?}, {moved:?}, {other:?}"),
    )
}

//! The [`SealingKeys`] contract suite. Every function creates the keys it
//! needs, so the caller passes a new, empty store.

use super::{Clause, RunTogether, Violation, ensure, unexpected};
use crate::{KEY_GRACE, KEY_ROTATE_AFTER, KeyBytes, KeyId, SealingKeys, Timestamp};

const START: u64 = 5_000_000;

/// The standard fixture; the suite needs no values of its own.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct KeyFixture;

impl KeyFixture {
    /// The standard fixture.
    ///
    /// # Errors
    ///
    /// Never; the signature matches the other suites' fixtures.
    pub fn standard() -> Result<Self, Violation> {
        Ok(Self)
    }
}

fn bytes(tag: u8) -> KeyBytes {
    KeyBytes::new([tag; 32])
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

/// The first call writes key 1 from its bytes, and every later call
/// answers that key and discards its own bytes.
///
/// # Errors
///
/// Returns [`Clause::TheFirstCallWritesKeyOne`] otherwise.
pub async fn the_first_call_writes_key_one<S: SealingKeys>(
    store: &S,
    _fixture: &KeyFixture,
) -> Result<(), Violation> {
    let first = unexpected(store.current(bytes(1), at(START)).await)?;
    let second = unexpected(store.current(bytes(2), at(START + 5)).await)?;
    ensure(
        first.id == KeyId::FIRST
            && first.bytes == bytes(1)
            && first.created == at(START)
            && second == first,
        Clause::TheFirstCallWritesKeyOne,
        || format!("first {first:?}, second {second:?}"),
    )
}

/// A rotation before the newest key is 30 days old changes nothing, and
/// one at 30 days writes the next id.
///
/// # Errors
///
/// Returns [`Clause::RotationWaitsThirtyDays`] otherwise.
pub async fn rotation_waits_thirty_days<S: SealingKeys>(
    store: &S,
    _fixture: &KeyFixture,
) -> Result<(), Violation> {
    let first = unexpected(store.current(bytes(1), at(START)).await)?;
    let early = unexpected(
        store
            .rotate(bytes(2), at(START + KEY_ROTATE_AFTER - 1))
            .await,
    )?;
    let due = unexpected(store.rotate(bytes(3), at(START + KEY_ROTATE_AFTER)).await)?;
    let again = unexpected(
        store
            .rotate(bytes(4), at(START + KEY_ROTATE_AFTER + 1))
            .await,
    )?;
    let current = unexpected(
        store
            .current(bytes(5), at(START + KEY_ROTATE_AFTER + 2))
            .await,
    )?;
    ensure(
        early == first
            && due.id.get() == 2
            && due.bytes == bytes(3)
            && due.created == at(START + KEY_ROTATE_AFTER)
            && again == due
            && current == due,
        Clause::RotationWaitsThirtyDays,
        || format!("early {early:?}, due {due:?}, again {again:?}, current {current:?}"),
    )
}

/// An old key opens for 1 hour after the next key was written, and not
/// from then on, while the newest key always opens.
///
/// # Errors
///
/// Returns [`Clause::AnOldKeyReadsForOneHourAfterRotation`] otherwise.
pub async fn an_old_key_reads_for_one_hour_after_rotation<S: SealingKeys>(
    store: &S,
    _fixture: &KeyFixture,
) -> Result<(), Violation> {
    let first = unexpected(store.current(bytes(1), at(START)).await)?;
    let rotated_at = START + KEY_ROTATE_AFTER;
    let second = unexpected(store.rotate(bytes(2), at(rotated_at)).await)?;
    let grace = unexpected(store.get(first.id, at(rotated_at + KEY_GRACE - 1)).await)?;
    let over = unexpected(store.get(first.id, at(rotated_at + KEY_GRACE)).await)?;
    let newest = unexpected(store.get(second.id, at(rotated_at + 100 * KEY_GRACE)).await)?;
    let unknown = unexpected(
        store
            .get(KeyId::new(9).unwrap_or(KeyId::FIRST), at(START))
            .await,
    )?;
    ensure(
        grace.as_ref() == Some(&first)
            && over.is_none()
            && newest.as_ref() == Some(&second)
            && unknown.is_none(),
        Clause::AnOldKeyReadsForOneHourAfterRotation,
        || format!("grace {grace:?}, over {over:?}, newest {newest:?}, unknown {unknown:?}"),
    )
}

/// Two calls that run together agree on one key: the first two `current`
/// calls on an empty store answer one key 1, and two rotations at the same
/// moment answer one new key and advance the id by one.
///
/// # Errors
///
/// Returns [`Clause::ConcurrentCallsAgreeOnOneKey`] otherwise.
pub async fn concurrent_calls_agree_on_one_key<S: SealingKeys>(
    store: &S,
    _fixture: &KeyFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    let answers = runner
        .run_together(
            || store.current(bytes(1), at(START)),
            || store.current(bytes(2), at(START)),
        )
        .await;
    let (one, two) = unexpected(answers.ok_or("a concurrent call panicked"))?;
    let (one, two) = (unexpected(one)?, unexpected(two)?);
    ensure(
        one == two && one.id == KeyId::FIRST,
        Clause::ConcurrentCallsAgreeOnOneKey,
        || format!("first call {one:?}, second call {two:?}"),
    )?;
    let mut last = one.id;
    for round in 1..8_u64 {
        let now = at(START + round * KEY_ROTATE_AFTER);
        let answers = runner
            .run_together(
                || store.rotate(bytes(3), now),
                || store.rotate(bytes(4), now),
            )
            .await;
        let (one, two) = unexpected(answers.ok_or("a concurrent rotation panicked"))?;
        let (one, two) = (unexpected(one)?, unexpected(two)?);
        ensure(
            one == two && Some(one.id) == last.next(),
            Clause::ConcurrentCallsAgreeOnOneKey,
            || format!("round {round}: {one:?} and {two:?} after {last:?}"),
        )?;
        last = one.id;
    }
    Ok(())
}

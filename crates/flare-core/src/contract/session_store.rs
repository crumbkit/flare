//! The [`SessionStore`] contract suite. Every function creates the sessions
//! it needs, so the caller passes a new, empty store.

use super::{Clause, People, Violation, ensure, unexpected};
use crate::{
    Digest, NewSession, Principal, SESSION_CAP, SESSION_IDLE, SessionRule, SessionStore, Timestamp,
};

const START: u64 = 1_000_000;
const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;

/// The people that the suite signs in.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SessionFixture {
    person: Principal,
    other: Principal,
}

impl SessionFixture {
    /// The standard fixture.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by its
    /// type, which means this crate is broken.
    pub fn standard() -> Result<Self, Violation> {
        Self::of(People::Emails)
    }

    /// The fixture with `people` as its principals. A username principal
    /// carries a verified address beside its username.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by its
    /// type, which means this crate is broken.
    pub fn of(people: People) -> Result<Self, Violation> {
        Ok(Self {
            person: people.principal("person")?,
            other: people.principal("other")?,
        })
    }

    fn session(&self, tag: u8) -> NewSession {
        NewSession::new(digest(tag), self.person.clone())
    }
}

fn digest(tag: u8) -> Digest {
    Digest::from_bytes([tag; 32])
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

fn rule() -> SessionRule {
    SessionRule::default()
}

/// A session reads back as its principal until 12 hours after it was
/// created, and is unknown from then on.
///
/// # Errors
///
/// Returns [`Clause::ASessionLivesTwelveHoursFromSignIn`] otherwise.
pub async fn a_session_lives_twelve_hours_from_sign_in<S: SessionStore>(
    store: &S,
    contract_fixture: &SessionFixture,
) -> Result<(), Violation> {
    let expires = unexpected(
        store
            .create(contract_fixture.session(1), None, &rule(), at(START))
            .await,
    )?;
    let early = unexpected(
        store
            .principal(&digest(1), &rule(), at(START + 5 * MINUTE))
            .await,
    )?;
    let last = unexpected(store.expiry(&digest(1), at(START + SESSION_IDLE - 1)).await)?;
    let end = unexpected(
        store
            .principal(&digest(1), &rule(), at(START + SESSION_IDLE))
            .await,
    )?;
    let unknown = unexpected(store.principal(&digest(9), &rule(), at(START)).await)?;
    ensure(
        expires == at(START + SESSION_IDLE)
            && early.as_ref() == Some(&contract_fixture.person)
            && last == Some(at(START + SESSION_IDLE))
            && end.is_none()
            && unknown.is_none(),
        Clause::ASessionLivesTwelveHoursFromSignIn,
        || format!("expires {expires:?}, early {early:?}, last {last:?}, end {end:?}"),
    )
}

/// A request less than 10 minutes into the idle life leaves the expiry
/// where it was, and a request after 10 minutes moves it to 12 hours from
/// that request.
///
/// # Errors
///
/// Returns [`Clause::ARequestSlidesTheExpiryAtMostEveryTenMinutes`]
/// otherwise.
pub async fn a_request_slides_the_expiry_at_most_every_ten_minutes<S: SessionStore>(
    store: &S,
    contract_fixture: &SessionFixture,
) -> Result<(), Violation> {
    unexpected(
        store
            .create(contract_fixture.session(1), None, &rule(), at(START))
            .await,
    )?;
    unexpected(
        store
            .principal(&digest(1), &rule(), at(START + 9 * MINUTE))
            .await,
    )?;
    let inside = unexpected(store.expiry(&digest(1), at(START + 9 * MINUTE)).await)?;
    unexpected(
        store
            .principal(&digest(1), &rule(), at(START + 11 * MINUTE))
            .await,
    )?;
    let after = unexpected(store.expiry(&digest(1), at(START + 11 * MINUTE)).await)?;
    let idle = unexpected(
        store
            .principal(&digest(1), &rule(), at(START + 11 * MINUTE + SESSION_IDLE))
            .await,
    )?;
    ensure(
        inside == Some(at(START + SESSION_IDLE))
            && after == Some(at(START + 11 * MINUTE + SESSION_IDLE))
            && idle.is_none(),
        Clause::ARequestSlidesTheExpiryAtMostEveryTenMinutes,
        || format!("inside {inside:?}, after {after:?}, idle {idle:?}"),
    )
}

/// A session used every 11 hours stops sliding at 7 days after sign-in,
/// and is unknown from then on.
///
/// # Errors
///
/// Returns [`Clause::ASessionEndsSevenDaysAfterSignIn`] otherwise.
pub async fn a_session_ends_seven_days_after_sign_in<S: SessionStore>(
    store: &S,
    contract_fixture: &SessionFixture,
) -> Result<(), Violation> {
    unexpected(
        store
            .create(contract_fixture.session(1), None, &rule(), at(START))
            .await,
    )?;
    let mut now = START;
    while now + 11 * HOUR < START + SESSION_CAP {
        now += 11 * HOUR;
        unexpected(store.principal(&digest(1), &rule(), at(now)).await)?;
    }
    let expiry = unexpected(store.expiry(&digest(1), at(now)).await)?;
    let before = unexpected(
        store
            .principal(&digest(1), &rule(), at(START + SESSION_CAP - 1))
            .await,
    )?;
    let end = unexpected(
        store
            .principal(&digest(1), &rule(), at(START + SESSION_CAP))
            .await,
    )?;
    ensure(
        expiry == Some(at(START + SESSION_CAP)) && before.is_some() && end.is_none(),
        Clause::ASessionEndsSevenDaysAfterSignIn,
        || format!("expiry {expiry:?}, before {before:?}, end {end:?}"),
    )
}

/// A person's ninth session ends the one with the earliest expiry, and
/// another person's sessions are unaffected.
///
/// # Errors
///
/// Returns [`Clause::APersonHoldsEightSessions`] otherwise.
pub async fn a_person_holds_eight_sessions<S: SessionStore>(
    store: &S,
    contract_fixture: &SessionFixture,
) -> Result<(), Violation> {
    let rest = NewSession::new(digest(100), contract_fixture.other.clone());
    unexpected(store.create(rest, None, &rule(), at(START)).await)?;
    for tag in 1..=9_u8 {
        let now = at(START + u64::from(tag) * MINUTE);
        unexpected(
            store
                .create(contract_fixture.session(tag), None, &rule(), now)
                .await,
        )?;
    }
    let now = at(START + 10 * MINUTE);
    let first = unexpected(store.principal(&digest(1), &rule(), now).await)?;
    let second = unexpected(store.principal(&digest(2), &rule(), now).await)?;
    let ninth = unexpected(store.principal(&digest(9), &rule(), now).await)?;
    let other = unexpected(store.principal(&digest(100), &rule(), now).await)?;
    ensure(
        first.is_none() && second.is_some() && ninth.is_some() && other.is_some(),
        Clause::APersonHoldsEightSessions,
        || format!("first {first:?}, second {second:?}, ninth {ninth:?}, other {other:?}"),
    )
}

/// A sign-in that names the browser's old session ends it in the step that
/// creates the new one, and an old session that is not live changes
/// nothing.
///
/// # Errors
///
/// Returns [`Clause::ASignInReplacesTheOldSession`] otherwise.
pub async fn a_sign_in_replaces_the_old_session<S: SessionStore>(
    store: &S,
    contract_fixture: &SessionFixture,
) -> Result<(), Violation> {
    let now = at(START);
    unexpected(
        store
            .create(contract_fixture.session(1), None, &rule(), now)
            .await,
    )?;
    unexpected(
        store
            .create(contract_fixture.session(2), Some(digest(1)), &rule(), now)
            .await,
    )?;
    unexpected(
        store
            .create(contract_fixture.session(3), Some(digest(77)), &rule(), now)
            .await,
    )?;
    let old = unexpected(store.principal(&digest(1), &rule(), now).await)?;
    let new = unexpected(store.principal(&digest(2), &rule(), now).await)?;
    let third = unexpected(store.principal(&digest(3), &rule(), now).await)?;
    ensure(
        old.is_none() && new.is_some() && third.is_some(),
        Clause::ASignInReplacesTheOldSession,
        || format!("old {old:?}, new {new:?}, third {third:?}"),
    )
}

/// A removed session is unknown, a removal of an unknown session changes
/// nothing, and removing every session of one person leaves another's.
///
/// # Errors
///
/// Returns [`Clause::RemovingEndsSessions`] otherwise.
pub async fn removing_ends_sessions<S: SessionStore>(
    store: &S,
    contract_fixture: &SessionFixture,
) -> Result<(), Violation> {
    let now = at(START);
    for tag in 1..=3_u8 {
        unexpected(
            store
                .create(contract_fixture.session(tag), None, &rule(), now)
                .await,
        )?;
    }
    let other = NewSession::new(digest(100), contract_fixture.other.clone());
    unexpected(store.create(other, None, &rule(), now).await)?;
    unexpected(store.remove(&digest(1)).await)?;
    unexpected(store.remove(&digest(55)).await)?;
    let one = unexpected(store.principal(&digest(1), &rule(), now).await)?;
    let two = unexpected(store.principal(&digest(2), &rule(), now).await)?;
    unexpected(store.remove_all_of(contract_fixture.person.id()).await)?;
    let gone = unexpected(store.principal(&digest(2), &rule(), now).await)?;
    let kept = unexpected(store.principal(&digest(100), &rule(), now).await)?;
    ensure(
        one.is_none() && two.is_some() && gone.is_none() && kept.is_some(),
        Clause::RemovingEndsSessions,
        || format!("one {one:?}, two {two:?}, gone {gone:?}, kept {kept:?}"),
    )
}

/// A person holds a session while one is live, and not after the last
/// expires or is removed.
///
/// # Errors
///
/// Returns [`Clause::HoldsFollowsLiveSessions`] otherwise.
pub async fn holds_follows_live_sessions<S: SessionStore>(
    store: &S,
    contract_fixture: &SessionFixture,
) -> Result<(), Violation> {
    let person = contract_fixture.person.id();
    let before = unexpected(store.holds(person, at(START)).await)?;
    unexpected(
        store
            .create(contract_fixture.session(1), None, &rule(), at(START))
            .await,
    )?;
    let live = unexpected(store.holds(person, at(START + HOUR)).await)?;
    let other = unexpected(
        store
            .holds(contract_fixture.other.id(), at(START + HOUR))
            .await,
    )?;
    let expired = unexpected(store.holds(person, at(START + SESSION_IDLE)).await)?;
    unexpected(store.remove(&digest(1)).await)?;
    let removed = unexpected(store.holds(person, at(START + HOUR)).await)?;
    ensure(
        !before && live && !other && !expired && !removed,
        Clause::HoldsFollowsLiveSessions,
        || {
            format!(
                "before {before:?}, live {live:?}, other {other:?}, expired {expired:?}, removed {removed:?}"
            )
        },
    )
}

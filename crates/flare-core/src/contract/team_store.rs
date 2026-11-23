//! The [`TeamStore`] contract suite.
//!
//! Every function writes the teams it needs through the store, so the
//! caller passes a new, empty store.

use super::{Clause, Violation, ensure, fixture as parsed, unexpected};
use crate::{Email, MAX_MEMBERS, Team, TeamName, TeamState, TeamStore, TeamStoreError, Timestamp};

/// The people and the team name that the suite uses.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TeamFixture {
    owner: Email,
    bob: Email,
    carol: Email,
    name: TeamName,
}

impl TeamFixture {
    /// The standard fixture.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by its
    /// type, which means this crate is broken.
    pub fn standard() -> Result<Self, Violation> {
        Ok(Self {
            owner: parsed(Email::try_from("owner@example.com"))?,
            bob: parsed(Email::try_from("bob@example.com"))?,
            carol: parsed(Email::try_from("carol@example.com"))?,
            name: parsed(TeamName::try_from("design"))?,
        })
    }

    /// The fixture's team, new and active.
    fn team(&self) -> Team {
        Team::new(
            self.name.clone(),
            self.owner.clone(),
            Timestamp::from_secs(1),
            TeamState::Active,
            1,
        )
    }
}

/// Creates the fixture's team.
async fn created<S: TeamStore>(store: &S, fixture: &TeamFixture) -> Result<(), Violation> {
    unexpected(store.create(&fixture.team()).await)
}

/// The team's member count as the store reads it.
async fn count<S: TeamStore>(store: &S, name: &TeamName) -> Result<Option<usize>, Violation> {
    Ok(unexpected(store.get(name).await)?.map(|team| team.members()))
}

/// A created team reads back active, with its owner as its only member.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::ACreatedTeamReadsBack`].
pub async fn a_created_team_reads_back_with_its_owner_as_its_only_member<S: TeamStore>(
    store: &S,
    fixture: &TeamFixture,
) -> Result<(), Violation> {
    created(store, fixture).await?;
    let read = unexpected(store.get(&fixture.name).await)?;
    let clause = Clause::ACreatedTeamReadsBack;
    ensure(read.as_ref() == Some(&fixture.team()), clause, || {
        format!("{read:?}")
    })?;
    let members = unexpected(store.members(&fixture.name).await)?;
    ensure(members == vec![fixture.owner.clone()], clause, || {
        format!("{} members", members.len())
    })?;
    let teams = unexpected(store.teams_of(&fixture.owner).await)?;
    ensure(teams == vec![fixture.name.clone()], clause, || {
        format!("{teams:?}")
    })
}

/// A used name is taken, even after its team is deleted.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::AUsedTeamNameIsTaken`].
pub async fn a_used_name_is_taken_even_after_its_team_is_deleted<S: TeamStore>(
    store: &S,
    fixture: &TeamFixture,
) -> Result<(), Violation> {
    created(store, fixture).await?;
    let other = Team::new(
        fixture.name.clone(),
        fixture.bob.clone(),
        Timestamp::from_secs(2),
        TeamState::Active,
        1,
    );
    let clause = Clause::AUsedTeamNameIsTaken;
    let again = store.create(&other).await;
    ensure(again == Err(TeamStoreError::Taken), clause, || {
        format!("{again:?}")
    })?;
    unexpected(store.mark_deleted(&fixture.name).await)?;
    let after = store.create(&other).await;
    ensure(after == Err(TeamStoreError::Taken), clause, || {
        format!("{after:?}")
    })?;
    let held = unexpected(store.get(&fixture.name).await)?;
    let owner = held.as_ref().map(Team::owner);
    ensure(owner == Some(&fixture.owner), clause, || {
        "the refused create replaced the team".to_owned()
    })?;
    let bobs = unexpected(store.teams_of(&fixture.bob).await)?;
    ensure(bobs.is_empty(), clause, || {
        "the refused create wrote a member pair".to_owned()
    })
}

/// Adds and removes leave both lists of every pair in agreement.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::MemberPairsAgree`].
pub async fn member_pairs_always_agree_after_adds_and_removes<S: TeamStore>(
    store: &S,
    fixture: &TeamFixture,
) -> Result<(), Violation> {
    created(store, fixture).await?;
    let name = &fixture.name;
    unexpected(store.add(name, &fixture.bob).await)?;
    unexpected(store.add(name, &fixture.carol).await)?;
    unexpected(store.remove(name, &fixture.bob).await)?;
    let clause = Clause::MemberPairsAgree;
    let members = unexpected(store.members(name).await)?;
    let expected = vec![fixture.carol.clone(), fixture.owner.clone()];
    ensure(members == expected, clause, || {
        format!("{} members", members.len())
    })?;
    let bobs = unexpected(store.teams_of(&fixture.bob).await)?;
    let carols = unexpected(store.teams_of(&fixture.carol).await)?;
    ensure(
        bobs.is_empty() && carols == vec![name.clone()],
        clause,
        || format!("bob {bobs:?}, carol {carols:?}"),
    )?;
    let counted = count(store, name).await?;
    ensure(counted == Some(2), clause, || format!("count {counted:?}"))
}

/// Adding a present member answers false and writes nothing.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::APresentMemberIsNotAddedTwice`].
pub async fn adding_a_present_member_writes_nothing<S: TeamStore>(
    store: &S,
    fixture: &TeamFixture,
) -> Result<(), Violation> {
    created(store, fixture).await?;
    let clause = Clause::APresentMemberIsNotAddedTwice;
    let first = unexpected(store.add(&fixture.name, &fixture.bob).await)?;
    let second = unexpected(store.add(&fixture.name, &fixture.bob).await)?;
    ensure(first && !second, clause, || format!("{first}, {second}"))?;
    let counted = count(store, &fixture.name).await?;
    ensure(counted == Some(2), clause, || format!("count {counted:?}"))
}

/// Removing an absent member answers false and writes nothing.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::AnAbsentMemberIsNotRemoved`].
pub async fn removing_an_absent_member_writes_nothing<S: TeamStore>(
    store: &S,
    fixture: &TeamFixture,
) -> Result<(), Violation> {
    created(store, fixture).await?;
    let removed = unexpected(store.remove(&fixture.name, &fixture.bob).await)?;
    let counted = count(store, &fixture.name).await?;
    ensure(
        !removed && counted == Some(1),
        Clause::AnAbsentMemberIsNotRemoved,
        || format!("{removed}, count {counted:?}"),
    )
}

/// A full team refuses another member and writes nothing.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::AFullTeamRefusesAMember`].
pub async fn a_full_team_refuses_another_member_and_writes_nothing<S: TeamStore>(
    store: &S,
    fixture: &TeamFixture,
) -> Result<(), Violation> {
    created(store, fixture).await?;
    for number in 1..MAX_MEMBERS {
        let member = parsed(Email::try_from(
            format!("member-{number}@example.com").as_str(),
        ))?;
        unexpected(store.add(&fixture.name, &member).await)?;
    }
    let clause = Clause::AFullTeamRefusesAMember;
    let refused = store.add(&fixture.name, &fixture.bob).await;
    ensure(refused == Err(TeamStoreError::Full), clause, || {
        format!("{refused:?}")
    })?;
    let bobs = unexpected(store.teams_of(&fixture.bob).await)?;
    let counted = count(store, &fixture.name).await?;
    ensure(
        bobs.is_empty() && counted == Some(MAX_MEMBERS),
        clause,
        || format!("bob {bobs:?}, count {counted:?}"),
    )
}

/// A deleted or missing team refuses a member and writes nothing.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::AnInactiveTeamRefusesAMember`].
pub async fn a_deleted_or_missing_team_refuses_a_member<S: TeamStore>(
    store: &S,
    fixture: &TeamFixture,
) -> Result<(), Violation> {
    let clause = Clause::AnInactiveTeamRefusesAMember;
    let missing = store.add(&fixture.name, &fixture.bob).await;
    ensure(missing == Err(TeamStoreError::NotFound), clause, || {
        format!("missing {missing:?}")
    })?;
    created(store, fixture).await?;
    unexpected(store.mark_deleted(&fixture.name).await)?;
    let deleted = store.add(&fixture.name, &fixture.bob).await;
    ensure(deleted == Err(TeamStoreError::NotFound), clause, || {
        format!("deleted {deleted:?}")
    })?;
    let bobs = unexpected(store.teams_of(&fixture.bob).await)?;
    ensure(bobs.is_empty(), clause, || format!("{bobs:?}"))
}

/// A deleted team keeps its owner, and marking it again changes nothing.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::ADeletedTeamKeepsItsOwner`].
pub async fn a_deleted_team_keeps_its_owner_and_its_state<S: TeamStore>(
    store: &S,
    fixture: &TeamFixture,
) -> Result<(), Violation> {
    created(store, fixture).await?;
    unexpected(store.mark_deleted(&fixture.name).await)?;
    unexpected(store.mark_deleted(&fixture.name).await)?;
    let held = unexpected(store.get(&fixture.name).await)?;
    let clause = Clause::ADeletedTeamKeepsItsOwner;
    ensure(
        held.as_ref().is_some_and(|team| {
            team.state() == TeamState::Deleted && team.owner() == &fixture.owner
        }),
        clause,
        || format!("{held:?}"),
    )?;
    let missing = parsed(TeamName::try_from("missing"))?;
    let refused = store.mark_deleted(&missing).await;
    ensure(refused == Err(TeamStoreError::NotFound), clause, || {
        format!("{refused:?}")
    })
}

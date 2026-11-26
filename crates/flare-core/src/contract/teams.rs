//! The [`Teams`] contract suite, for every source of teams: flare's own
//! store, a deployment's directory, or none.
//!
//! The caller builds its implementation holding exactly the teams that
//! [`TeamsFixture::teams`] lists, each with its members, and passes a new
//! one to each function. A source with no teams at all runs the suite with
//! [`TeamsFixture::empty`].

use super::{Clause, People, Violation, ensure, fixture as parsed, unexpected};
use crate::{Person, TeamName, Teams, TeamsError, Timestamp};

/// The teams the caller seeds, and a person in none of them.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TeamsFixture {
    teams: Vec<(TeamName, Vec<Person>)>,
    outsider: Person,
}

impl TeamsFixture {
    /// The standard fixture, with people named by address.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by its
    /// type, which means this crate is broken.
    pub fn standard() -> Result<Self, Violation> {
        Self::of(People::Emails)
    }

    /// The teams `design`, with `owner` and `bob`, and `ops`, with `owner`,
    /// whose people are of `people`. The first member of each team is the
    /// one a store that keeps owners makes its owner.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by its
    /// type, which means this crate is broken.
    pub fn of(people: People) -> Result<Self, Violation> {
        let owner = people.person("owner")?;
        let bob = people.person("bob")?;
        Ok(Self {
            teams: vec![
                (
                    parsed(TeamName::try_from("design"))?,
                    vec![owner.clone(), bob],
                ),
                (parsed(TeamName::try_from("ops"))?, vec![owner]),
            ],
            outsider: people.person("carol")?,
        })
    }

    /// No teams, for a source that has none.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by its
    /// type, which means this crate is broken.
    pub fn empty(people: People) -> Result<Self, Violation> {
        Ok(Self {
            teams: Vec::new(),
            outsider: people.person("carol")?,
        })
    }

    /// The teams to seed, each with its members.
    #[must_use]
    pub fn teams(&self) -> &[(TeamName, Vec<Person>)] {
        &self.teams
    }

    /// Everyone the fixture names, members first.
    fn people(&self) -> Vec<Person> {
        let mut people: Vec<Person> = Vec::new();
        for (_, members) in &self.teams {
            for member in members {
                if !people.contains(member) {
                    people.push(member.clone());
                }
            }
        }
        people.push(self.outsider.clone());
        people
    }

    /// The names of the fixture's teams that `person` is in, in name order.
    fn teams_of(&self, person: &Person) -> Vec<TeamName> {
        let mut names: Vec<TeamName> = self
            .teams
            .iter()
            .filter(|(_, members)| members.contains(person))
            .map(|(name, _)| name.clone())
            .collect();
        names.sort();
        names
    }

    /// A name that no team of the fixture has.
    fn missing() -> Result<TeamName, Violation> {
        parsed(TeamName::try_from("nobody"))
    }
}

/// Sorts people by identifier.
fn sorted(mut people: Vec<Person>) -> Vec<Person> {
    people.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    people
}

/// `teams_of` and `members` agree for every person and every team.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::TeamsOfAndMembersAgree`].
pub async fn teams_of_and_members_agree<T: Teams>(
    teams: &T,
    fixture: &TeamsFixture,
) -> Result<(), Violation> {
    let clause = Clause::TeamsOfAndMembersAgree;
    for person in fixture.people() {
        for name in unexpected(teams.teams_of(&person).await)? {
            if !unexpected(teams.exists(&name).await)? {
                continue;
            }
            let members = unexpected(teams.members(&name).await)?;
            ensure(members.contains(&person), clause, || {
                format!("{name} is among the person's teams and leaves them out of its members")
            })?;
        }
    }
    for (name, _) in fixture.teams() {
        for member in unexpected(teams.members(name).await)? {
            let theirs = unexpected(teams.teams_of(&member).await)?;
            ensure(theirs.contains(name), clause, || {
                format!("a member of {name} does not name it among their teams")
            })?;
        }
    }
    Ok(())
}

/// Every team that a person is in exists, and holds the fixture's members.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::ExistsHoldsForEveryTeamOfAPerson`].
pub async fn exists_holds_for_every_team_a_person_is_in<T: Teams>(
    teams: &T,
    fixture: &TeamsFixture,
) -> Result<(), Violation> {
    let clause = Clause::ExistsHoldsForEveryTeamOfAPerson;
    for (name, members) in fixture.teams() {
        let exists = unexpected(teams.exists(name).await)?;
        ensure(exists, clause, || format!("{name} does not exist"))?;
        let held = sorted(unexpected(teams.members(name).await)?);
        ensure(held == sorted(members.clone()), clause, || {
            format!("{name} has {} members", held.len())
        })?;
    }
    let missing = TeamsFixture::missing()?;
    let exists = unexpected(teams.exists(&missing).await)?;
    ensure(!exists, clause, || format!("{missing} exists"))?;
    let members = teams.members(&missing).await;
    ensure(members == Err(TeamsError::NotFound), clause, || {
        format!("the members of a missing team answered {members:?}")
    })
}

/// A list gives the caller's teams in name order with their member counts.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::ListGivesTheCallersTeams`].
pub async fn list_gives_the_callers_active_teams_in_name_order<T: Teams>(
    teams: &T,
    fixture: &TeamsFixture,
) -> Result<(), Violation> {
    let clause = Clause::ListGivesTheCallersTeams;
    for person in fixture.people() {
        let listed = unexpected(teams.list(&person).await)?;
        let names: Vec<TeamName> = listed.iter().map(|team| team.name().clone()).collect();
        let wanted = fixture.teams_of(&person);
        ensure(names == wanted, clause, || format!("listed {names:?}"))?;
        for team in &listed {
            let count = fixture
                .teams()
                .iter()
                .find(|(name, _)| name == team.name())
                .map_or(0, |(_, members)| members.len());
            ensure(team.members() == count, clause, || {
                format!("{} lists {} members", team.name(), team.members())
            })?;
        }
    }
    Ok(())
}

/// A show answers a member and refuses everyone else alike.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::ShowKeepsTheVisibilityRule`].
pub async fn show_gives_the_members_to_a_member_and_not_found_to_anyone_else<T: Teams>(
    teams: &T,
    fixture: &TeamsFixture,
) -> Result<(), Violation> {
    let clause = Clause::ShowKeepsTheVisibilityRule;
    let people = fixture.people();
    for (name, members) in fixture.teams() {
        for person in &people {
            let shown = teams.show(person, name).await;
            if members.contains(person) {
                let (team, listed) = unexpected(shown)?;
                ensure(team.name() == name, clause, || format!("showed {team:?}"))?;
                ensure(listed == sorted(members.clone()), clause, || {
                    format!("{name} showed {} members", listed.len())
                })?;
            } else {
                ensure(
                    shown.is_err_and(|e| e == TeamsError::NotFound),
                    clause,
                    || format!("{name} was shown to a person outside it"),
                )?;
            }
        }
    }
    let missing = TeamsFixture::missing()?;
    for person in &people {
        let shown = teams.show(person, &missing).await;
        ensure(
            shown.is_err_and(|e| e == TeamsError::NotFound),
            clause,
            || "a missing team was shown".to_owned(),
        )?;
    }
    Ok(())
}

/// What every person's teams and every team's members are, to compare
/// before and after the changes.
async fn snapshot<T: Teams>(
    teams: &T,
    fixture: &TeamsFixture,
) -> Result<Vec<(Vec<TeamName>, Vec<Person>)>, Violation> {
    let mut seen = Vec::new();
    for person in fixture.people() {
        let theirs = unexpected(teams.teams_of(&person).await)?;
        seen.push((theirs, Vec::new()));
    }
    for (name, _) in fixture.teams() {
        let members = sorted(unexpected(teams.members(name).await)?);
        seen.push((vec![name.clone()], members));
    }
    Ok(seen)
}

/// When flare does not manage the source, every change answers
/// [`TeamsError::ManagedElsewhere`] and writes nothing. A source that flare
/// manages passes without a check here; the store's own suite covers it.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::AnUnmanagedSourceChangesNothing`].
pub async fn a_source_flare_does_not_manage_answers_managed_elsewhere_and_changes_nothing<
    T: Teams,
>(
    teams: &T,
    fixture: &TeamsFixture,
) -> Result<(), Violation> {
    if teams.manages() {
        return Ok(());
    }
    let clause = Clause::AnUnmanagedSourceChangesNothing;
    let before = snapshot(teams, fixture).await?;
    let outsider = &fixture.outsider;
    let missing = TeamsFixture::missing()?;
    let elsewhere = Err(TeamsError::ManagedElsewhere);
    let created = teams
        .create(outsider, missing.clone(), Timestamp::from_secs(1))
        .await
        .map(|_| ());
    ensure(created == elsewhere, clause, || {
        format!("create {created:?}")
    })?;
    for (name, members) in fixture.teams() {
        let Some(first) = members.first() else {
            continue;
        };
        let added = teams.add(first, name, outsider).await;
        ensure(added == Err(TeamsError::ManagedElsewhere), clause, || {
            format!("add {added:?}")
        })?;
        let removed = teams.remove(first, name, first).await;
        ensure(removed == Err(TeamsError::ManagedElsewhere), clause, || {
            format!("remove {removed:?}")
        })?;
        let moved = teams.transfer(first, name, outsider).await.map(|_| ());
        ensure(moved == elsewhere, clause, || format!("transfer {moved:?}"))?;
        let deleted = teams.delete(first, name).await;
        ensure(deleted == elsewhere, clause, || {
            format!("delete {deleted:?}")
        })?;
        let finished = teams.finish_delete(first, name).await;
        ensure(finished == elsewhere, clause, || {
            format!("finish delete {finished:?}")
        })?;
    }
    let after = snapshot(teams, fixture).await?;
    ensure(before == after, clause, || {
        "the teams changed after the refused changes".to_owned()
    })?;
    let exists = unexpected(teams.exists(&missing).await)?;
    ensure(!exists, clause, || {
        "a refused create made a team".to_owned()
    })
}

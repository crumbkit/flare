//! Runs the `Teams` suite against the three team sources that flare ships:
//! `FlareTeams` on the in-memory store, `NoTeams` and `DocumentTeams`, with
//! people named by address and by username.
#![cfg(test)]

use flare_core::contract::teams::{self, TeamsFixture};
use flare_core::contract::{People, block_on};
use flare_core::memory::MemoryTeamStore;
use flare_core::{
    DocumentTeams, DocumentUnreadable, FlareTeams, IdentityKind, NoTeams, Teams, TeamsDocument,
    Timestamp,
};
use serde_json::{Map, Value, json};

/// The identifier kind of `people`.
fn kind(people: People) -> IdentityKind {
    match people {
        People::Usernames => IdentityKind::Username,
        _ => IdentityKind::Email,
    }
}

/// Stored teams holding the fixture's teams: the first member of each
/// creates it, then adds the others.
fn stored(fixture: &TeamsFixture) -> FlareTeams<MemoryTeamStore> {
    let teams = FlareTeams::new(MemoryTeamStore::new());
    for (name, members) in fixture.teams() {
        let (owner, rest) = members.split_first().unwrap();
        block_on(teams.create(owner, name.clone(), Timestamp::from_secs(1))).unwrap();
        for member in rest {
            assert!(block_on(teams.add(owner, name, member)).unwrap());
        }
    }
    teams
}

/// A document that holds fixed bytes.
struct Fixed(Vec<u8>);

impl TeamsDocument for Fixed {
    async fn read(&self) -> Result<Vec<u8>, DocumentUnreadable> {
        Ok(self.0.clone())
    }
}

/// Directory teams read from a document of the fixture's teams.
fn directory(fixture: &TeamsFixture, people: People) -> DocumentTeams<Fixed> {
    let teams: Map<String, Value> = fixture
        .teams()
        .iter()
        .map(|(name, members)| {
            let members: Vec<&str> = members.iter().map(|member| member.as_str()).collect();
            (name.as_str().to_owned(), json!(members))
        })
        .collect();
    let bytes = serde_json::to_vec(&json!({ "teams": teams })).unwrap();
    block_on(DocumentTeams::load(Fixed(bytes), kind(people))).unwrap()
}

macro_rules! suite {
    ($($name:ident),* $(,)?) => {$(
        mod $name {
            use super::*;

            #[test]
            fn passes_for_stored_teams() {
                for people in People::ALL {
                    let fixture = TeamsFixture::of(people).unwrap();
                    block_on(teams::$name(&stored(&fixture), &fixture)).unwrap();
                }
            }

            #[test]
            fn passes_for_no_teams() {
                for people in People::ALL {
                    let fixture = TeamsFixture::empty(people).unwrap();
                    block_on(teams::$name(&NoTeams, &fixture)).unwrap();
                }
            }

            #[test]
            fn passes_for_directory_teams() {
                for people in People::ALL {
                    let fixture = TeamsFixture::of(people).unwrap();
                    block_on(teams::$name(&directory(&fixture, people), &fixture)).unwrap();
                }
            }
        }
    )*};
}

suite!(
    teams_of_and_members_agree,
    exists_holds_for_every_team_a_person_is_in,
    list_gives_the_callers_active_teams_in_name_order,
    show_gives_the_members_to_a_member_and_not_found_to_anyone_else,
    a_source_flare_does_not_manage_answers_managed_elsewhere_and_changes_nothing,
);

/// Stored teams with one member pair that only one side of the pair
/// knows: the person's teams name the team, and the team's members leave
/// the person out.
struct OneSided(FlareTeams<MemoryTeamStore>, flare_core::Person);

impl Teams for OneSided {
    fn manages(&self) -> bool {
        true
    }

    async fn teams_of(
        &self,
        person: &flare_core::Person,
    ) -> Result<Vec<flare_core::TeamName>, flare_core::TeamsError> {
        self.0.teams_of(person).await
    }

    async fn exists(&self, team: &flare_core::TeamName) -> Result<bool, flare_core::TeamsError> {
        self.0.exists(team).await
    }

    async fn members(
        &self,
        team: &flare_core::TeamName,
    ) -> Result<Vec<flare_core::Person>, flare_core::TeamsError> {
        let mut members = self.0.members(team).await?;
        members.retain(|member| member != &self.1);
        Ok(members)
    }
}

#[test]
fn the_suite_catches_teams_of_and_members_that_disagree() {
    let fixture = TeamsFixture::standard().unwrap();
    let (_, members) = &fixture.teams()[0];
    let broken = OneSided(stored(&fixture), members[1].clone());
    let violation = block_on(teams::teams_of_and_members_agree(&broken, &fixture)).unwrap_err();
    assert_eq!(
        violation.clause(),
        flare_core::contract::Clause::TeamsOfAndMembersAgree
    );
}

/// Read-only teams whose create answers success, which a source that flare
/// does not manage must never do.
struct WritesAnyway(DocumentTeams<Fixed>);

impl Teams for WritesAnyway {
    async fn teams_of(
        &self,
        person: &flare_core::Person,
    ) -> Result<Vec<flare_core::TeamName>, flare_core::TeamsError> {
        self.0.teams_of(person).await
    }

    async fn exists(&self, team: &flare_core::TeamName) -> Result<bool, flare_core::TeamsError> {
        self.0.exists(team).await
    }

    async fn members(
        &self,
        team: &flare_core::TeamName,
    ) -> Result<Vec<flare_core::Person>, flare_core::TeamsError> {
        self.0.members(team).await
    }

    async fn add(
        &self,
        _actor: &flare_core::Person,
        _team: &flare_core::TeamName,
        _member: &flare_core::Person,
    ) -> Result<bool, flare_core::TeamsError> {
        Ok(true)
    }
}

#[test]
fn the_suite_catches_a_source_flare_does_not_manage_that_accepts_a_change() {
    let fixture = TeamsFixture::standard().unwrap();
    let broken = WritesAnyway(directory(&fixture, People::Emails));
    let violation = block_on(
        teams::a_source_flare_does_not_manage_answers_managed_elsewhere_and_changes_nothing(
            &broken, &fixture,
        ),
    )
    .unwrap_err();
    assert_eq!(
        violation.clause(),
        flare_core::contract::Clause::AnUnmanagedSourceChangesNothing
    );
}

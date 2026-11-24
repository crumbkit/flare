//! The activity index for team and everyone grants: which group entries
//! each write leaves, as the exact set of (reader, response, flare), and
//! what a team member's activity lists through them.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::memory::{
    MemoryGrantStore, MemoryResponseStore, MemoryShareStore, MemoryTeamStore,
};
use flare_core::{
    ActivityPage, Admission, Creators, DocumentText, Email, FlareError, FlareTeams, Flares, Group,
    MailDomain, NewFlare, Position, Principal, PrincipalId, Reader, ShareContent, ShareId,
    ShareTitle, TeamName, Teams, Timestamp,
};

type Memory =
    Flares<MemoryShareStore, MemoryGrantStore, MemoryResponseStore, FlareTeams<MemoryTeamStore>>;

fn email(name: &str) -> Email {
    Email::try_from(format!("{name}@example.com").as_str()).unwrap()
}

fn person(name: &str) -> Principal {
    let address = format!("{name}@example.com");
    Principal::with_email(
        PrincipalId::try_from(address.as_str()).unwrap(),
        email(name),
    )
}

fn team(name: &str) -> TeamName {
    TeamName::try_from(name).unwrap()
}

fn design() -> Group {
    Group::Team(team("design"))
}

fn id(byte: u8) -> ShareId {
    ShareId::from_random([byte; 16])
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

fn flare(byte: u8, with: &[&str], groups: Vec<Group>, re: &[u8]) -> NewFlare {
    NewFlare::new(
        id(byte),
        ShareTitle::try_from(format!("Flare {byte}").as_str()).unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        with.iter().map(|name| email(name)).collect(),
        re.iter().map(|byte| id(*byte)).collect(),
    )
    .with_groups(groups)
}

/// A service in which alice owns the team design, with bob and carol as
/// members, and everyone at example.com is admitted.
fn with_design() -> Memory {
    let flares = Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::new(),
        Creators::Everyone,
    )
    .with_teams(FlareTeams::new(MemoryTeamStore::new()))
    .admitting(Admission::Domain(
        MailDomain::try_from("example.com").unwrap(),
    ));
    block_on(flares.create_team(&person("alice"), team("design"), at(1))).unwrap();
    let teams = flares.shares().teams();
    for member in ["bob", "carol"] {
        block_on(teams.add(&email("alice"), &team("design"), &email(member))).unwrap();
    }
    flares
}

fn share(flares: &Memory, who: &str, byte: u8, with: &[&str], groups: Vec<Group>, re: &[u8]) {
    block_on(flares.share(&person(who), flare(byte, with, groups, re), at(2))).unwrap();
}

/// Every index entry as (reader, response, flare), sorted, with a person
/// named without the domain and a group by its grantee id.
fn entries(flares: &Memory) -> Vec<(String, ShareId, ShareId)> {
    let mut found: Vec<_> = flares
        .responses()
        .entries()
        .into_iter()
        .map(|entry| {
            let name = match entry.reader {
                Reader::Person(reader) => {
                    reader.as_str().trim_end_matches("@example.com").to_owned()
                }
                Reader::Group(group) => group.to_string(),
            };
            (name, entry.response, entry.flare)
        })
        .collect();
    found.sort();
    found
}

fn expect(list: &[(&str, u8, u8)]) -> Vec<(String, ShareId, ShareId)> {
    let mut wanted: Vec<_> = list
        .iter()
        .map(|(name, response, flare)| ((*name).to_owned(), id(*response), id(*flare)))
        .collect();
    wanted.sort();
    wanted
}

fn activity(flares: &Memory, who: &str, byte: u8) -> Result<ActivityPage, FlareError> {
    block_on(flares.activity(&person(who), &[id(byte)], Position::new(0), 50))
}

fn shown(page: &ActivityPage) -> Vec<ShareId> {
    page.items
        .iter()
        .map(|item| item.response.id().clone())
        .collect()
}

#[test]
fn a_response_shared_with_the_flares_team_is_indexed_once_for_the_team_and_every_member_sees_it() {
    let flares = with_design();
    share(&flares, "alice", 1, &[], vec![design()], &[]);
    share(&flares, "bob", 2, &[], vec![design()], &[1]);
    // Alice owns the flare and is granted the response as its owner; the
    // team holds both. Bob and carol hold the flare only through the team,
    // so they have no entry of their own.
    assert_eq!(
        entries(&flares),
        expect(&[("alice", 2, 1), ("team:design", 2, 1)])
    );
    for member in ["alice", "bob", "carol"] {
        let page = activity(&flares, member, 1).unwrap();
        assert_eq!(shown(&page), vec![id(2)], "{member}");
    }
}

#[test]
fn a_member_removed_from_the_team_stops_seeing_its_activity_with_no_rewrite_of_entries() {
    let flares = with_design();
    share(&flares, "alice", 1, &[], vec![design()], &[]);
    share(&flares, "bob", 2, &[], vec![design()], &[1]);
    let before = entries(&flares);
    let teams = flares.shares().teams();
    block_on(teams.remove(&email("alice"), &team("design"), &email("carol"))).unwrap();
    assert_eq!(entries(&flares), before);
    assert_eq!(
        activity(&flares, "carol", 1).err(),
        Some(FlareError::NotFound)
    );
}

#[test]
fn granting_a_response_to_a_team_indexes_it_for_the_team_on_each_flare_the_team_can_open() {
    let flares = with_design();
    share(&flares, "alice", 1, &[], vec![design()], &[]);
    share(&flares, "alice", 3, &[], Vec::new(), &[]);
    share(&flares, "bob", 2, &[], Vec::new(), &[1]);
    assert_eq!(entries(&flares), expect(&[("alice", 2, 1)]));
    block_on(flares.grant_all(&person("bob"), &id(2), &[], &[design()], &[], at(3))).unwrap();
    assert_eq!(
        entries(&flares),
        expect(&[("alice", 2, 1), ("team:design", 2, 1)])
    );
    assert_eq!(shown(&activity(&flares, "carol", 1).unwrap()), vec![id(2)]);
}

#[test]
fn granting_a_flare_to_a_team_indexes_the_responses_the_team_already_holds() {
    let flares = with_design();
    share(&flares, "alice", 1, &["bob"], Vec::new(), &[]);
    share(&flares, "bob", 2, &[], vec![design()], &[1]);
    // The team holds the response and not yet the flare.
    assert_eq!(entries(&flares), expect(&[("alice", 2, 1), ("bob", 2, 1)]));
    block_on(flares.grant_all(&person("alice"), &id(1), &[], &[design()], &[], at(3))).unwrap();
    assert_eq!(
        entries(&flares),
        expect(&[("alice", 2, 1), ("bob", 2, 1), ("team:design", 2, 1)])
    );
    assert_eq!(shown(&activity(&flares, "carol", 1).unwrap()), vec![id(2)]);
}

#[test]
fn an_everyone_grant_on_both_shares_indexes_one_everyone_entry_that_anyone_admitted_reads() {
    let flares = with_design();
    share(&flares, "alice", 1, &[], vec![Group::Everyone], &[]);
    share(&flares, "bob", 2, &[], vec![Group::Everyone], &[1]);
    assert_eq!(
        entries(&flares),
        expect(&[("alice", 2, 1), ("everyone", 2, 1)])
    );
    // Dave belongs to no team and holds no grant.
    assert_eq!(shown(&activity(&flares, "dave", 1).unwrap()), vec![id(2)]);
}

#[test]
fn revoking_a_teams_grant_on_a_response_removes_the_teams_entries_for_it() {
    let flares = with_design();
    share(&flares, "alice", 1, &[], vec![design()], &[]);
    share(&flares, "bob", 2, &[], vec![design()], &[1]);
    block_on(flares.revoke(&person("bob"), &id(2), &[], &[design()])).unwrap();
    assert_eq!(entries(&flares), expect(&[("alice", 2, 1)]));
    assert!(activity(&flares, "carol", 1).unwrap().items.is_empty());
}

#[test]
fn a_response_a_member_reaches_as_a_person_and_through_the_team_is_listed_once() {
    let flares = with_design();
    share(&flares, "alice", 1, &[], vec![design()], &[]);
    share(&flares, "bob", 2, &[], vec![design()], &[1]);
    // Alice holds an entry of her own and reads the team's entry too.
    assert_eq!(shown(&activity(&flares, "alice", 1).unwrap()), vec![id(2)]);
}

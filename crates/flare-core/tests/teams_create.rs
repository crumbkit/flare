//! Creating a team: a deployment whose teams are managed elsewhere refuses
//! the create before it changes any grant, and a create of a free name
//! removes at most [`MAX_SWEPT`] grants left under the name in one call.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemorySequence,
    MemoryShareStore, MemoryTeamStore,
};
use flare_core::{
    Admission, CommentStore, Creators, CursorStore, DocumentText, Email, FlareTeams, Flares,
    GrantStore, Group, MAX_SWEPT, MailDomain, NewFlare, Principal, PrincipalId, ResponseStore,
    ShareContent, ShareId, ShareStore, ShareTitle, TeamName, Teams, TeamsError, Timestamp,
};
use std::sync::atomic::{AtomicBool, Ordering};

/// A read-only directory of teams that holds design, with alice and bob as
/// its members, while `design` is set. It writes nothing, so every change
/// answers [`TeamsError::ManagedElsewhere`].
struct Directory {
    design: AtomicBool,
}

impl Directory {
    fn holds(&self, name: &TeamName) -> bool {
        *name == team("design") && self.design.load(Ordering::SeqCst)
    }
}

impl Teams for Directory {
    async fn teams_of(&self, person: &Email) -> Result<Vec<TeamName>, TeamsError> {
        let member = [email("alice"), email("bob")].contains(person);
        Ok(if member && self.holds(&team("design")) {
            vec![team("design")]
        } else {
            Vec::new()
        })
    }

    async fn exists(&self, name: &TeamName) -> Result<bool, TeamsError> {
        Ok(self.holds(name))
    }

    async fn members(&self, name: &TeamName) -> Result<Vec<Email>, TeamsError> {
        if self.holds(name) {
            Ok(vec![email("alice"), email("bob")])
        } else {
            Err(TeamsError::NotFound)
        }
    }
}

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

fn id(n: u16) -> ShareId {
    let [high, low] = n.to_be_bytes();
    let mut bytes = [5_u8; 16];
    bytes[0] = high;
    bytes[1] = low;
    ShareId::from_random(bytes)
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

fn flare(n: u16) -> NewFlare {
    NewFlare::new(
        id(n),
        ShareTitle::try_from(format!("Flare {n}").as_str()).unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        Vec::new(),
        Vec::new(),
    )
    .with_groups(vec![design()])
}

/// A service over the in-memory stores and `teams`, in which everyone is a
/// creator.
fn service<T: Teams>(
    teams: T,
) -> Flares<
    MemoryShareStore,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
    T,
> {
    let sequence = MemorySequence::new();
    Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::with_sequence(sequence.clone()),
        MemoryCommentStore::with_sequence(sequence),
        MemoryCursorStore::new(),
        Creators::Everyone,
    )
    .with_teams(teams)
    .admitting(Admission::Domain(
        MailDomain::try_from("example.com").unwrap(),
    ))
}

/// How many of alice's shares 1 to `count` list a grant to design.
fn design_grants<
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    C: CommentStore,
    U: CursorStore,
    T: Teams,
>(
    flares: &Flares<S, G, R, C, U, T>,
    count: u16,
) -> usize {
    (1..=count)
        .filter(|n| {
            block_on(flares.shares().grants(&person("alice"), &id(*n)))
                .unwrap()
                .iter()
                .any(|grant| grant.grantee() == &design().principal_id())
        })
        .count()
}

#[test]
fn a_create_on_a_deployment_whose_teams_are_managed_elsewhere_keeps_grants_left_under_the_name() {
    let flares = service(Directory {
        design: AtomicBool::new(true),
    });
    block_on(flares.share(&person("alice"), flare(1), at(1))).unwrap();
    // The directory drops design; the grant to it stays on alice's share.
    flares
        .shares()
        .teams()
        .design
        .store(false, Ordering::SeqCst);
    let refused = block_on(flares.create_team(&person("carol"), team("design"), at(2)));
    assert_eq!(refused.unwrap_err(), TeamsError::ManagedElsewhere);
    assert_eq!(design_grants(&flares, 1), 1);
}

#[test]
fn a_create_that_finds_more_leftover_grants_than_one_call_removes_creates_on_a_repeat() {
    let flares = service(FlareTeams::new(MemoryTeamStore::new()));
    block_on(flares.create_team(&person("alice"), team("design"), at(1))).unwrap();
    let count = u16::try_from(MAX_SWEPT + 1).unwrap();
    for n in 1..=count {
        block_on(flares.share(&person("alice"), flare(n), at(2))).unwrap();
    }
    // The team goes without its grants being removed, as a delete whose
    // removals failed after the name was freed would leave it.
    let teams = flares.shares().teams();
    block_on(teams.delete(&email("alice"), &team("design"))).unwrap();
    block_on(teams.finish_delete(&email("alice"), &team("design"))).unwrap();
    assert_eq!(design_grants(&flares, count), MAX_SWEPT + 1);

    let first = block_on(flares.create_team(&person("carol"), team("design"), at(3)));
    assert_eq!(first.unwrap_err(), TeamsError::Unavailable);
    assert_eq!(design_grants(&flares, count), 1);
    assert!(!block_on(teams.taken(&team("design"))).unwrap());

    let created = block_on(flares.create_team(&person("carol"), team("design"), at(4))).unwrap();
    assert_eq!(created.owner(), &email("carol"));
    assert_eq!(design_grants(&flares, count), 0);
}

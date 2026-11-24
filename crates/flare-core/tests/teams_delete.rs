//! Deleting a team removes its grants from every share and frees its
//! name, a delete that stops midway finishes on a repeat, and an owner
//! hands a team to another member.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemorySequence,
    MemoryShareStore, MemoryTeamStore,
};
use flare_core::{
    Admission, Creators, DocumentText, Email, Grant, GrantRevoke, GrantStore, GrantStoreError,
    GrantWrite, Group, ListKey, Listing, MAX_SWEPT, MailDomain, NewFlare, Position, Principal,
    PrincipalId, ShareContent, ShareError, ShareId, ShareTitle, Team, TeamName, Teams, TeamsError,
    Timestamp,
};
use flare_core::{FlareTeams, Flares};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// A grant store whose revokes fail once `left` reaches zero; each revoke
/// that passes counts `left` down. `usize::MAX` never fails.
struct RevokesRunOut {
    inner: Arc<MemoryGrantStore>,
    left: Arc<AtomicUsize>,
}

impl GrantStore for RevokesRunOut {
    async fn grant(&self, write: GrantWrite) -> Result<Grant, GrantStoreError> {
        self.inner.grant(write).await
    }

    async fn revoke(&self, revoke: GrantRevoke) -> Result<(), GrantStoreError> {
        let left = self.left.load(Ordering::SeqCst);
        if left == 0 {
            return Err(GrantStoreError::Unavailable);
        }
        if left != usize::MAX {
            self.left.store(left - 1, Ordering::SeqCst);
        }
        self.inner.revoke(revoke).await
    }

    async fn get(
        &self,
        share: &ShareId,
        grantee: &PrincipalId,
    ) -> Result<Option<Grant>, GrantStoreError> {
        self.inner.get(share, grantee).await
    }

    async fn of_share(&self, share: &ShareId) -> Result<Vec<Grant>, GrantStoreError> {
        self.inner.of_share(share).await
    }

    async fn granted_page(
        &self,
        grantee: &PrincipalId,
        after: Option<&ListKey>,
        limit: usize,
    ) -> Result<Listing<Grant>, GrantStoreError> {
        self.inner.granted_page(grantee, after, limit).await
    }
}

/// The default teams, which delete design for alice right after the next
/// read of whether a team is active, while `race` is set. A grant reads
/// that between its first check and its write.
struct Racing {
    inner: FlareTeams<MemoryTeamStore>,
    race: AtomicBool,
}

impl Teams for Racing {
    async fn teams_of(&self, person: &Email) -> Result<Vec<TeamName>, TeamsError> {
        self.inner.teams_of(person).await
    }

    async fn exists(&self, name: &TeamName) -> Result<bool, TeamsError> {
        self.inner.exists(name).await
    }

    async fn taken(&self, name: &TeamName) -> Result<bool, TeamsError> {
        self.inner.taken(name).await
    }

    async fn active(&self, name: &TeamName) -> Result<Option<Timestamp>, TeamsError> {
        let found = self.inner.active(name).await;
        if self.race.swap(false, Ordering::SeqCst) {
            self.inner.delete(&email("alice"), &team("design")).await?;
            self.inner
                .finish_delete(&email("alice"), &team("design"))
                .await?;
        }
        found
    }

    async fn members(&self, name: &TeamName) -> Result<Vec<Email>, TeamsError> {
        self.inner.members(name).await
    }

    async fn create(
        &self,
        owner: &Email,
        name: TeamName,
        at: Timestamp,
    ) -> Result<Team, TeamsError> {
        self.inner.create(owner, name, at).await
    }

    async fn add(
        &self,
        actor: &Email,
        name: &TeamName,
        member: &Email,
    ) -> Result<bool, TeamsError> {
        self.inner.add(actor, name, member).await
    }

    async fn remove(
        &self,
        actor: &Email,
        name: &TeamName,
        member: &Email,
    ) -> Result<bool, TeamsError> {
        self.inner.remove(actor, name, member).await
    }

    async fn delete(&self, actor: &Email, name: &TeamName) -> Result<(), TeamsError> {
        self.inner.delete(actor, name).await
    }

    async fn finish_delete(&self, actor: &Email, name: &TeamName) -> Result<(), TeamsError> {
        self.inner.finish_delete(actor, name).await
    }

    async fn transfer(
        &self,
        actor: &Email,
        name: &TeamName,
        to: &Email,
    ) -> Result<Team, TeamsError> {
        self.inner.transfer(actor, name, to).await
    }

    async fn show(&self, actor: &Email, name: &TeamName) -> Result<(Team, Vec<Email>), TeamsError> {
        self.inner.show(actor, name).await
    }

    async fn list(&self, actor: &Email) -> Result<Vec<Team>, TeamsError> {
        self.inner.list(actor).await
    }
}

type Service = Flares<
    MemoryShareStore,
    RevokesRunOut,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
    Racing,
>;

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
    let mut bytes = [7_u8; 16];
    bytes[0] = high;
    bytes[1] = low;
    ShareId::from_random(bytes)
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

fn flare(n: u16, groups: Vec<Group>, re: &[u16]) -> NewFlare {
    NewFlare::new(
        id(n),
        ShareTitle::try_from(format!("Flare {n}").as_str()).unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        Vec::new(),
        re.iter().map(|n| id(*n)).collect(),
    )
    .with_groups(groups)
}

/// A service in which alice owns design with bob as a member, and whose
/// revokes fail after `left` of them.
fn with_design(left: Arc<AtomicUsize>) -> Service {
    with_design_over(left, Arc::new(MemoryGrantStore::new()))
}

/// The same service over `grants`, which the test also writes.
fn with_design_over(left: Arc<AtomicUsize>, grants: Arc<MemoryGrantStore>) -> Service {
    let sequence = MemorySequence::new();
    let flares = Flares::new(
        MemoryShareStore::new(),
        RevokesRunOut {
            inner: grants,
            left,
        },
        MemoryResponseStore::with_sequence(sequence.clone()),
        MemoryCommentStore::with_sequence(sequence),
        MemoryCursorStore::new(),
        Creators::Everyone,
    )
    .with_teams(Racing {
        inner: FlareTeams::new(MemoryTeamStore::new()),
        race: AtomicBool::new(false),
    })
    .admitting(Admission::Domain(
        MailDomain::try_from("example.com").unwrap(),
    ));
    block_on(flares.create_team(&person("alice"), team("design"), at(1))).unwrap();
    let teams = flares.shares().teams();
    block_on(teams.add(&email("alice"), &team("design"), &email("bob"))).unwrap();
    flares
}

fn never_fails() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(usize::MAX))
}

fn share(flares: &Service, who: &str, n: u16, groups: Vec<Group>, re: &[u16]) {
    block_on(flares.share(&person(who), flare(n, groups, re), at(2))).unwrap();
}

fn opens(flares: &Service, who: &str, n: u16) -> Result<(), ShareError> {
    block_on(flares.shares().open(&person(who), &id(n), None)).map(|_| ())
}

/// The grantee ids on share `n`, as its owner `owner` lists them, sorted.
fn grantees(flares: &Service, owner: &str, n: u16) -> Vec<String> {
    let mut found: Vec<String> = block_on(flares.shares().grants(&person(owner), &id(n)))
        .unwrap()
        .iter()
        .map(|grant| grant.grantee().as_str().to_owned())
        .collect();
    found.sort();
    found
}

/// The readers of every index entry, sorted.
fn readers(flares: &Service) -> Vec<String> {
    let mut found: Vec<String> = flares
        .responses()
        .entries()
        .into_iter()
        .map(|entry| entry.reader.grantee().as_str().to_owned())
        .collect();
    found.sort();
    found
}

#[test]
fn deleting_a_team_removes_its_grant_from_every_share_and_its_activity_entries() {
    let flares = with_design(never_fails());
    share(&flares, "alice", 1, vec![design()], &[]);
    share(&flares, "bob", 2, vec![design()], &[1]);
    share(&flares, "alice", 3, vec![design(), Group::Everyone], &[]);
    let deleted = block_on(flares.delete_team(&person("alice"), &team("design"))).unwrap();
    assert!(deleted.finished);
    assert_eq!(deleted.removed, 3);
    assert_eq!(grantees(&flares, "alice", 1), Vec::<String>::new());
    assert_eq!(grantees(&flares, "bob", 2), vec!["alice@example.com"]);
    assert_eq!(grantees(&flares, "alice", 3), vec!["everyone"]);
    assert_eq!(readers(&flares), vec!["alice@example.com"]);
    assert_eq!(opens(&flares, "bob", 1), Err(ShareError::NotFound));
    let teams = flares.shares().teams();
    assert_eq!(block_on(teams.teams_of(&email("bob"))).unwrap(), vec![]);
}

#[test]
fn a_deleted_teams_name_is_free_and_a_new_team_of_that_name_gets_none_of_the_old_access() {
    let flares = with_design(never_fails());
    share(&flares, "alice", 1, vec![design()], &[]);
    block_on(flares.delete_team(&person("alice"), &team("design"))).unwrap();
    let created = block_on(flares.create_team(&person("carol"), team("design"), at(9))).unwrap();
    assert_eq!(created.owner(), &email("carol"));
    assert_eq!(opens(&flares, "carol", 1), Err(ShareError::NotFound));
    assert_eq!(opens(&flares, "bob", 1), Err(ShareError::NotFound));
}

#[test]
fn a_delete_that_fails_midway_leaves_the_team_opening_nothing_and_a_repeat_finishes_it() {
    let left = never_fails();
    let flares = with_design(Arc::clone(&left));
    share(&flares, "alice", 1, vec![design()], &[]);
    share(&flares, "alice", 2, vec![design()], &[]);
    left.store(1, Ordering::SeqCst);
    let failed = block_on(flares.delete_team(&person("alice"), &team("design")));
    assert_eq!(failed.unwrap_err(), TeamsError::Unavailable);
    // One grant is gone, one is left, and the team opens neither.
    assert_eq!(
        [grantees(&flares, "alice", 1), grantees(&flares, "alice", 2)].concat(),
        vec!["team:design"]
    );
    assert_eq!(opens(&flares, "bob", 1), Err(ShareError::NotFound));
    assert_eq!(opens(&flares, "bob", 2), Err(ShareError::NotFound));
    // The name stays taken, and only the owner finishes the delete.
    let taken = block_on(flares.create_team(&person("carol"), team("design"), at(9)));
    assert_eq!(taken.unwrap_err(), TeamsError::Taken);
    let stranger = block_on(flares.delete_team(&person("bob"), &team("design")));
    assert_eq!(stranger.unwrap_err(), TeamsError::NotFound);
    left.store(usize::MAX, Ordering::SeqCst);
    let repeated = block_on(flares.delete_team(&person("alice"), &team("design"))).unwrap();
    assert!(repeated.finished);
    assert_eq!(repeated.removed, 1);
    assert_eq!(
        [grantees(&flares, "alice", 1), grantees(&flares, "alice", 2)].concat(),
        Vec::<String>::new()
    );
    block_on(flares.create_team(&person("carol"), team("design"), at(9))).unwrap();
}

#[test]
fn a_team_with_more_grants_than_one_call_removes_is_deleted_over_repeated_calls() {
    let flares = with_design(never_fails());
    let count = u16::try_from(MAX_SWEPT + 1).unwrap();
    for n in 1..=count {
        share(&flares, "alice", n, vec![design()], &[]);
    }
    let first = block_on(flares.delete_team(&person("alice"), &team("design"))).unwrap();
    assert!(!first.finished);
    assert_eq!(first.removed, MAX_SWEPT);
    let second = block_on(flares.delete_team(&person("alice"), &team("design"))).unwrap();
    assert!(second.finished);
    assert_eq!(second.removed, 1);
}

#[test]
fn a_team_grant_that_finds_the_team_deleted_after_its_write_removes_itself() {
    let flares = with_design(never_fails());
    share(&flares, "alice", 1, Vec::new(), &[]);
    flares.shares().teams().race.store(true, Ordering::SeqCst);
    let granted = block_on(
        flares
            .shares()
            .grant_group(&person("alice"), &id(1), &design(), at(3)),
    );
    assert_eq!(granted.unwrap_err(), ShareError::TeamNotFound);
    assert_eq!(grantees(&flares, "alice", 1), Vec::<String>::new());
}

#[test]
fn a_grant_left_under_a_name_with_no_team_is_removed_before_a_new_team_takes_the_name() {
    let left = never_fails();
    let flares = with_design(Arc::clone(&left));
    share(&flares, "alice", 1, Vec::new(), &[]);
    // The grant's own removal fails too, so the grant stays under a name
    // that no team has.
    flares.shares().teams().race.store(true, Ordering::SeqCst);
    left.store(0, Ordering::SeqCst);
    let granted = block_on(
        flares
            .shares()
            .grant_group(&person("alice"), &id(1), &design(), at(3)),
    );
    assert_eq!(granted.unwrap_err(), ShareError::Unavailable);
    assert_eq!(grantees(&flares, "alice", 1), vec!["team:design"]);
    left.store(usize::MAX, Ordering::SeqCst);
    block_on(flares.create_team(&person("carol"), team("design"), at(9))).unwrap();
    assert_eq!(grantees(&flares, "alice", 1), Vec::<String>::new());
    assert_eq!(opens(&flares, "carol", 1), Err(ShareError::NotFound));
}

#[test]
fn the_owner_transfers_a_team_to_a_member_who_then_manages_it_alone() {
    let flares = with_design(never_fails());
    let teams = flares.shares().teams();
    let moved = block_on(teams.transfer(&email("alice"), &team("design"), &email("bob"))).unwrap();
    assert_eq!(moved.owner(), &email("bob"));
    assert_eq!(moved.members(), 2);
    let by_old = block_on(teams.add(&email("alice"), &team("design"), &email("carol")));
    assert_eq!(by_old, Err(TeamsError::Forbidden));
    assert_eq!(
        block_on(teams.add(&email("bob"), &team("design"), &email("carol"))),
        Ok(true)
    );
    // The former owner is an ordinary member and may leave.
    assert_eq!(
        block_on(teams.remove(&email("alice"), &team("design"), &email("alice"))),
        Ok(true)
    );
    let deleted = block_on(flares.delete_team(&person("bob"), &team("design"))).unwrap();
    assert!(deleted.finished);
}

#[test]
fn a_transfer_needs_the_owner_and_a_member_and_a_repeat_answers_the_team_as_it_is() {
    let flares = with_design(never_fails());
    let teams = flares.shares().teams();
    let by_member = block_on(teams.transfer(&email("bob"), &team("design"), &email("bob")));
    assert_eq!(by_member, Err(TeamsError::Forbidden));
    let to_stranger = block_on(teams.transfer(&email("alice"), &team("design"), &email("dave")));
    assert_eq!(to_stranger, Err(TeamsError::NotAMember));
    let by_stranger = block_on(teams.transfer(&email("dave"), &team("design"), &email("bob")));
    assert_eq!(by_stranger, Err(TeamsError::NotFound));
    block_on(teams.transfer(&email("alice"), &team("design"), &email("bob"))).unwrap();
    let again = block_on(teams.transfer(&email("alice"), &team("design"), &email("bob"))).unwrap();
    assert_eq!(again.owner(), &email("bob"));
}

#[test]
fn a_transfer_to_a_person_who_owns_the_most_teams_is_refused() {
    let flares = with_design(never_fails());
    for n in 0..flare_core::MAX_TEAMS_OWNED {
        let name = team(&format!("bob-{n}"));
        block_on(flares.create_team(&person("bob"), name, at(2))).unwrap();
    }
    let teams = flares.shares().teams();
    let refused = block_on(teams.transfer(&email("alice"), &team("design"), &email("bob")));
    assert_eq!(refused, Err(TeamsError::TooManyOwned));
    let kept: Team = block_on(teams.show(&email("alice"), &team("design")))
        .unwrap()
        .0;
    assert_eq!(kept.owner(), &email("alice"));
}

#[test]
fn a_members_activity_through_the_team_ends_when_the_team_is_deleted() {
    let flares = with_design(never_fails());
    share(&flares, "alice", 1, vec![design(), Group::Everyone], &[]);
    share(&flares, "alice", 2, vec![design()], &[1]);
    let activity = |flares: &Service| {
        block_on(flares.activity(&person("bob"), &[id(1)], Position::new(0), 50))
            .unwrap()
            .items
            .len()
    };
    assert_eq!(activity(&flares), 1);
    block_on(flares.delete_team(&person("alice"), &team("design"))).unwrap();
    // Bob still opens the flare through everyone, and the response is
    // no longer listed to him.
    assert_eq!(activity(&flares), 0);
}

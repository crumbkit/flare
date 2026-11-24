//! Teams and group grants when a store fails: a group grant that fails
//! after the share exists, a team store that does not answer while a share
//! is opened, and a delete that stops before it removes the member pairs.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemorySequence,
    MemoryShareStore, MemoryTeamStore,
};
use flare_core::{
    Admission, CreatorList, DocumentText, Email, FlareError, FlareTeams, Flares, Grant,
    GrantRevoke, GrantStore, GrantStoreError, GrantWrite, Group, ListKey, Listing, MailDomain,
    NewFlare, Principal, PrincipalId, ShareContent, ShareError, ShareId, ShareTitle, Standing,
    Team, TeamName, TeamStore, TeamStoreError, Teams, TeamsError, Timestamp,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn email(text: &str) -> Email {
    Email::try_from(text).unwrap()
}

fn person(address: &str) -> Principal {
    Principal::with_email(PrincipalId::try_from(address).unwrap(), email(address))
}

fn alice() -> Principal {
    person("alice@example.com")
}

fn bob() -> Principal {
    person("bob@example.com")
}

fn team(name: &str) -> TeamName {
    TeamName::try_from(name).unwrap()
}

fn id(byte: u8) -> ShareId {
    ShareId::from_random([byte; 16])
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

fn creators() -> CreatorList {
    CreatorList::only(vec![email("alice@example.com"), email("bob@example.com")])
}

fn admission() -> Admission {
    Admission::Domain(MailDomain::try_from("example.com").unwrap())
}

fn flare(byte: u8, with: &[&str], groups: Vec<Group>) -> NewFlare {
    NewFlare::new(
        id(byte),
        ShareTitle::try_from(format!("Flare {byte}").as_str()).unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        with.iter().map(|text| email(text)).collect(),
        Vec::new(),
    )
    .with_groups(groups)
}

/// A grant store that refuses every group grant while `fail` is set.
#[derive(Default)]
struct GroupGrantsFail {
    inner: MemoryGrantStore,
    fail: Arc<AtomicBool>,
}

impl GrantStore for GroupGrantsFail {
    async fn grant(&self, write: GrantWrite) -> Result<Grant, GrantStoreError> {
        let group = !write.grantee().as_str().contains('@');
        if group && self.fail.load(Ordering::SeqCst) {
            return Err(GrantStoreError::Unavailable);
        }
        self.inner.grant(write).await
    }

    async fn revoke(&self, revoke: GrantRevoke) -> Result<(), GrantStoreError> {
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

#[test]
fn a_team_grant_that_fails_after_the_share_exists_answers_partly_shared_and_a_repeat_finishes() {
    let fail = Arc::new(AtomicBool::new(false));
    let grants = GroupGrantsFail {
        inner: MemoryGrantStore::new(),
        fail: Arc::clone(&fail),
    };
    let sequence = MemorySequence::new();
    let flares = Flares::new(
        MemoryShareStore::new(),
        grants,
        MemoryResponseStore::with_sequence(sequence.clone()),
        MemoryCommentStore::with_sequence(sequence),
        MemoryCursorStore::new(),
        creators(),
    )
    .with_teams(FlareTeams::new(MemoryTeamStore::new()))
    .admitting(admission());
    block_on(flares.create_team(&alice(), team("design"), at(1))).unwrap();
    fail.store(true, Ordering::SeqCst);
    let design = Group::Team(team("design"));
    let shared = block_on(flares.share(
        &alice(),
        flare(1, &["carol@example.com"], vec![design.clone()]),
        at(2),
    ));
    assert_eq!(
        shared.unwrap_err(),
        FlareError::PartlyShared {
            share: id(1),
            granted: 1,
            cause: ShareError::Unavailable,
        }
    );
    fail.store(false, Ordering::SeqCst);
    let finished = block_on(flares.grant_all(
        &alice(),
        &id(1),
        &[email("carol@example.com")],
        &[design],
        &[],
        at(3),
    ))
    .unwrap();
    assert!(finished.granted.is_empty());
    assert_eq!(finished.groups, vec![Group::Team(team("design"))]);
    let grantees: Vec<String> = block_on(flares.shares().grants(&alice(), &id(1)))
        .unwrap()
        .iter()
        .map(|grant| grant.grantee().as_str().to_owned())
        .collect();
    assert_eq!(grantees.len(), 2);
    assert!(grantees.contains(&"team:design".to_owned()));
}

/// Flare's teams, whose reads answer unavailable while `down` is set.
struct SwitchedTeams {
    inner: FlareTeams<MemoryTeamStore>,
    down: Arc<AtomicBool>,
}

impl SwitchedTeams {
    fn check(&self) -> Result<(), TeamsError> {
        if self.down.load(Ordering::SeqCst) {
            Err(TeamsError::Unavailable)
        } else {
            Ok(())
        }
    }
}

impl Teams for SwitchedTeams {
    fn manages(&self) -> bool {
        self.inner.manages()
    }

    async fn teams_of(&self, person: &Email) -> Result<Vec<TeamName>, TeamsError> {
        self.check()?;
        self.inner.teams_of(person).await
    }

    async fn exists(&self, team: &TeamName) -> Result<bool, TeamsError> {
        self.check()?;
        self.inner.exists(team).await
    }

    async fn members(&self, team: &TeamName) -> Result<Vec<Email>, TeamsError> {
        self.check()?;
        self.inner.members(team).await
    }

    async fn create(
        &self,
        owner: &Email,
        team: TeamName,
        at: Timestamp,
    ) -> Result<Team, TeamsError> {
        self.inner.create(owner, team, at).await
    }

    async fn add(
        &self,
        actor: &Email,
        team: &TeamName,
        member: &Email,
    ) -> Result<bool, TeamsError> {
        self.inner.add(actor, team, member).await
    }
}

#[test]
fn an_unavailable_team_store_answers_unavailable_to_a_reader_through_a_team_and_still_opens_a_direct_grant()
 {
    let down = Arc::new(AtomicBool::new(false));
    let teams = SwitchedTeams {
        inner: FlareTeams::new(MemoryTeamStore::new()),
        down: Arc::clone(&down),
    };
    let sequence = MemorySequence::new();
    let flares = Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::with_sequence(sequence.clone()),
        MemoryCommentStore::with_sequence(sequence),
        MemoryCursorStore::new(),
        creators(),
    )
    .with_teams(teams)
    .admitting(admission());
    block_on(flares.create_team(&alice(), team("design"), at(1))).unwrap();
    let shares = flares.shares();
    block_on(shares.teams().add(
        &email("alice@example.com"),
        &team("design"),
        &email("bob@example.com"),
    ))
    .unwrap();
    block_on(flares.share(
        &alice(),
        flare(1, &["carol@example.com"], vec![Group::Team(team("design"))]),
        at(2),
    ))
    .unwrap();
    assert_eq!(
        block_on(shares.open(&bob(), &id(1), None))
            .unwrap()
            .standing,
        Standing::Reader
    );
    down.store(true, Ordering::SeqCst);
    let refused = block_on(shares.open(&bob(), &id(1), None)).unwrap_err();
    assert_eq!(refused, ShareError::Unavailable);
    let carol = person("carol@example.com");
    let opened = block_on(shares.open(&carol, &id(1), None)).unwrap();
    assert_eq!(opened.standing, Standing::Reader);
}

/// A team store whose member removal fails while `fail` is set.
#[derive(Default)]
struct RemovalFails {
    inner: MemoryTeamStore,
    fail: Arc<AtomicBool>,
}

impl TeamStore for RemovalFails {
    async fn create(&self, team: &Team) -> Result<(), TeamStoreError> {
        self.inner.create(team).await
    }

    async fn get(&self, team: &TeamName) -> Result<Option<Team>, TeamStoreError> {
        self.inner.get(team).await
    }

    async fn add(&self, team: &TeamName, member: &Email) -> Result<bool, TeamStoreError> {
        self.inner.add(team, member).await
    }

    async fn remove(&self, team: &TeamName, member: &Email) -> Result<bool, TeamStoreError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(TeamStoreError::Unavailable);
        }
        self.inner.remove(team, member).await
    }

    async fn mark_deleting(&self, team: &TeamName) -> Result<(), TeamStoreError> {
        self.inner.mark_deleting(team).await
    }

    async fn forget(&self, team: &TeamName) -> Result<bool, TeamStoreError> {
        self.inner.forget(team).await
    }

    async fn transfer(
        &self,
        team: &TeamName,
        from: &Email,
        to: &Email,
    ) -> Result<(), TeamStoreError> {
        self.inner.transfer(team, from, to).await
    }

    async fn members(&self, team: &TeamName) -> Result<Vec<Email>, TeamStoreError> {
        self.inner.members(team).await
    }

    async fn teams_of(&self, person: &Email) -> Result<Vec<TeamName>, TeamStoreError> {
        self.inner.teams_of(person).await
    }
}

#[test]
fn a_delete_that_stops_before_the_member_pairs_leaves_the_team_deleting_and_a_repeat_frees_the_name()
 {
    let fail = Arc::new(AtomicBool::new(false));
    let store = RemovalFails {
        inner: MemoryTeamStore::new(),
        fail: Arc::clone(&fail),
    };
    let sequence = MemorySequence::new();
    let flares = Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::with_sequence(sequence.clone()),
        MemoryCommentStore::with_sequence(sequence),
        MemoryCursorStore::new(),
        creators(),
    )
    .with_teams(FlareTeams::new(store))
    .admitting(admission());
    let owner = email("alice@example.com");
    let member = email("bob@example.com");
    block_on(flares.create_team(&alice(), team("design"), at(1))).unwrap();
    let teams = flares.shares().teams();
    block_on(teams.add(&owner, &team("design"), &member)).unwrap();
    block_on(flares.share(
        &alice(),
        flare(1, &[], vec![Group::Team(team("design"))]),
        at(2),
    ))
    .unwrap();
    fail.store(true, Ordering::SeqCst);
    let stopped = block_on(flares.delete_team(&alice(), &team("design"))).unwrap_err();
    assert_eq!(stopped, TeamsError::Unavailable);
    assert_eq!(
        block_on(flares.shares().open(&bob(), &id(1), None)).unwrap_err(),
        ShareError::NotFound
    );
    assert!(!block_on(teams.teams_of(&member)).unwrap().is_empty());
    let taken = block_on(flares.create_team(&bob(), team("design"), at(3)));
    assert_eq!(taken.unwrap_err(), TeamsError::Taken);
    fail.store(false, Ordering::SeqCst);
    let finished = block_on(flares.delete_team(&alice(), &team("design"))).unwrap();
    assert!(finished.finished);
    assert!(block_on(teams.teams_of(&member)).unwrap().is_empty());
    assert!(block_on(teams.teams_of(&owner)).unwrap().is_empty());
    block_on(flares.create_team(&bob(), team("design"), at(3))).unwrap();
}

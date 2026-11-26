//! The creators seam over the in-memory stores: `Flares` asks an app's own
//! `Creators` only about a principal with a verified address, and a
//! `Creators` that cannot answer refuses a share or a team before any
//! write.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemorySequence,
    MemoryShareStore, MemoryTeamStore,
};
use flare_core::{
    AllAdmitted, Creators, CreatorsError, DocumentText, Email, FlareError, FlareTeams, Flares,
    NewFlare, Principal, PrincipalId, ShareContent, ShareError, ShareId, ShareTitle, TeamName,
    Teams, TeamsError, Timestamp,
};
use std::sync::atomic::{AtomicUsize, Ordering};

/// An app's creators rule that asks a directory, which answers with
/// `answer` and counts each question.
#[derive(Debug)]
struct Directory {
    answer: Result<bool, CreatorsError>,
    asked: AtomicUsize,
}

impl Directory {
    fn answering(answer: Result<bool, CreatorsError>) -> Self {
        Self {
            answer,
            asked: AtomicUsize::new(0),
        }
    }

    fn asked(&self) -> usize {
        self.asked.load(Ordering::SeqCst)
    }
}

impl Creators for &Directory {
    async fn may_create(&self, _who: &Principal) -> Result<bool, CreatorsError> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.answer
    }
}

type Memory<K> = Flares<
    MemoryShareStore,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
    FlareTeams<MemoryTeamStore>,
    K,
>;

fn stores<K>(creators: K) -> Memory<K> {
    let sequence = MemorySequence::new();
    Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::with_sequence(sequence.clone()),
        MemoryCommentStore::with_sequence(sequence),
        MemoryCursorStore::new(),
        creators,
    )
    .with_teams(FlareTeams::new(MemoryTeamStore::new()))
}

fn ana() -> Principal {
    let email = Email::try_from("ana@example.org").unwrap();
    Principal::with_email(PrincipalId::try_from("ana@example.org").unwrap(), email)
}

fn local() -> Principal {
    Principal::new(PrincipalId::try_from("local:ana").unwrap())
}

fn id() -> ShareId {
    ShareId::from_random([7; 16])
}

fn flare() -> NewFlare {
    NewFlare::new(
        id(),
        ShareTitle::try_from("Plan").unwrap(),
        ShareContent::Document(DocumentText::try_from("# Plan\n").unwrap()),
        Vec::new(),
        Vec::new(),
    )
}

fn design() -> TeamName {
    TeamName::try_from("design").unwrap()
}

fn at() -> Timestamp {
    Timestamp::from_secs(1)
}

#[test]
fn a_creators_rule_that_cannot_answer_refuses_a_share_and_writes_none() {
    let directory = Directory::answering(Err(CreatorsError::Unavailable));
    let flares = stores(&directory);
    let refused = block_on(flares.share(&ana(), flare(), at()));
    assert_eq!(refused.unwrap_err(), FlareError::CreatorsUnavailable);
    let opened = block_on(flares.shares().open(&ana(), &id(), None));
    assert_eq!(opened.unwrap_err(), ShareError::NotFound);
}

#[test]
fn a_creators_rule_that_cannot_answer_refuses_a_team_as_unavailable_and_writes_none() {
    let directory = Directory::answering(Err(CreatorsError::Unavailable));
    let flares = stores(&directory);
    let refused = block_on(flares.create_team(&ana(), design(), at()));
    assert_eq!(refused.unwrap_err(), TeamsError::Unavailable);
    assert!(!block_on(flares.shares().teams().taken(&design())).unwrap());
}

#[test]
fn a_creators_rule_that_answers_no_refuses_a_share_and_a_team_as_not_a_creator() {
    let directory = Directory::answering(Ok(false));
    let flares = stores(&directory);
    let share = block_on(flares.share(&ana(), flare(), at()));
    assert_eq!(share.unwrap_err(), FlareError::NotACreator);
    let team = block_on(flares.create_team(&ana(), design(), at()));
    assert_eq!(team.unwrap_err(), TeamsError::NotACreator);
}

#[test]
fn a_principal_without_a_verified_address_is_refused_before_the_creators_rule_is_asked() {
    let directory = Directory::answering(Ok(true));
    let flares = stores(&directory);
    let share = block_on(flares.share(&local(), flare(), at()));
    assert_eq!(share.unwrap_err(), FlareError::Share(ShareError::NoPerson));
    let team = block_on(flares.create_team(&local(), design(), at()));
    assert_eq!(team.unwrap_err(), TeamsError::NotACreator);
    assert_eq!(directory.asked(), 0);
}

#[test]
fn the_default_creators_rule_lets_every_person_with_a_verified_address_share_and_create_a_team() {
    let flares = stores(AllAdmitted);
    assert!(block_on(flares.share(&ana(), flare(), at())).is_ok());
    assert!(block_on(flares.create_team(&ana(), design(), at())).is_ok());
}

//! `DocumentTeams`: a deployment's read-only teams, read from a document
//! that its own sync job writes. Who lists and shows them, what each change
//! answers, which documents are refused, and when the document is read
//! again.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::{
    DOCUMENT_MAX_AGE, DocumentRefused, DocumentTeams, DocumentUnreadable, IdentityKind,
    MAX_DIRECTORY_MEMBERS, MAX_DIRECTORY_TEAMS, MAX_DOCUMENT_BYTES, MAX_TEAMS_JOINED, ManagedBy,
    Person, TeamName, Teams, TeamsDocument, TeamsError, Timestamp,
};
use serde_json::{Value, json};
use std::future::Future as _;
use std::pin::pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

/// What the next read of a [`Sync`] answers: the bytes, or why it fails.
type Next = Option<Result<Vec<u8>, String>>;

/// A document whose bytes a test sets, and which counts its reads.
#[derive(Clone, Default)]
struct Sync {
    held: Arc<Mutex<Next>>,
    reads: Arc<Mutex<usize>>,
}

impl Sync {
    fn holding(document: &Value) -> Self {
        let sync = Self::default();
        sync.write(document);
        sync
    }

    fn write(&self, document: &Value) {
        self.write_bytes(serde_json::to_vec(document).unwrap());
    }

    fn write_bytes(&self, bytes: Vec<u8>) {
        *self.held.lock().unwrap() = Some(Ok(bytes));
    }

    fn fail(&self, reason: &str) {
        *self.held.lock().unwrap() = Some(Err(reason.to_owned()));
    }

    fn reads(&self) -> usize {
        *self.reads.lock().unwrap()
    }
}

impl TeamsDocument for Sync {
    async fn read(&self) -> Result<Vec<u8>, DocumentUnreadable> {
        *self.reads.lock().unwrap() += 1;
        match self.held.lock().unwrap().clone() {
            Some(Ok(bytes)) => Ok(bytes),
            Some(Err(reason)) => Err(DocumentUnreadable::new(reason)),
            None => Err(DocumentUnreadable::new("no document")),
        }
    }
}

/// A [`Sync`] whose reads wait until the test opens its gate.
#[derive(Clone, Default)]
struct Slow {
    sync: Sync,
    gate: Arc<Mutex<Gate>>,
}

/// Whether a [`Slow`] read may finish, and the read that waits for it.
#[derive(Default)]
struct Gate {
    shut: bool,
    waiting: Option<Waker>,
}

impl Slow {
    fn shut(&self) {
        self.gate.lock().unwrap().shut = true;
    }

    fn open(&self) {
        let mut gate = self.gate.lock().unwrap();
        gate.shut = false;
        if let Some(waker) = gate.waiting.take() {
            waker.wake();
        }
    }
}

impl TeamsDocument for Slow {
    async fn read(&self) -> Result<Vec<u8>, DocumentUnreadable> {
        let read = self.sync.read().await;
        std::future::poll_fn(|context| {
            let mut gate = self.gate.lock().unwrap();
            if gate.shut {
                gate.waiting = Some(context.waker().clone());
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
        read
    }
}

/// A clock that a test moves forward.
#[derive(Clone)]
struct Clock(Arc<Mutex<Instant>>);

impl Clock {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Instant::now())))
    }

    fn advance(&self, by: Duration) {
        let mut now = self.0.lock().unwrap();
        *now += by;
    }

    fn reader(&self) -> impl Fn() -> Instant + Send + std::marker::Sync + 'static + use<> {
        let held = Arc::clone(&self.0);
        move || *held.lock().unwrap()
    }
}

fn email(text: &str) -> Person {
    Person::try_from(text).unwrap()
}

fn team(name: &str) -> TeamName {
    TeamName::try_from(name).unwrap()
}

fn design() -> Value {
    json!({"teams": {
        "design": ["bo@example.com", "ana@example.com"],
        "ops": ["ana@example.com"],
        "sales": ["cy@example.com"],
    }})
}

fn loaded(document: &Value) -> (DocumentTeams<Sync>, Sync) {
    let sync = Sync::holding(document);
    let teams = block_on(DocumentTeams::load(sync.clone(), IdentityKind::Email)).unwrap();
    (teams, sync)
}

fn refused(document: &[u8]) -> DocumentRefused {
    let sync = Sync::default();
    sync.write_bytes(document.to_vec());
    block_on(DocumentTeams::load(sync, IdentityKind::Email)).unwrap_err()
}

#[test]
fn a_member_lists_their_directory_teams_in_name_order_with_no_owner() {
    let (teams, _) = loaded(&design());
    let listed = block_on(teams.list(&email("ana@example.com"))).unwrap();
    let names: Vec<&str> = listed.iter().map(|team| team.name().as_str()).collect();
    assert_eq!(names, ["design", "ops"]);
    assert_eq!(listed[0].owner(), None);
    assert_eq!(listed[0].created(), None);
    assert_eq!(listed[0].managed_by(), &ManagedBy::Directory);
    assert_eq!(listed[0].members(), 2);
    assert!(
        block_on(teams.list(&email("dee@example.com")))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_member_sees_the_members_in_address_order_and_anyone_else_gets_not_found() {
    let (teams, _) = loaded(&design());
    let (shown, members) = block_on(teams.show(&email("bo@example.com"), &team("design"))).unwrap();
    assert_eq!(shown.name(), &team("design"));
    assert_eq!(shown.owner(), None);
    assert_eq!(
        members,
        vec![email("ana@example.com"), email("bo@example.com")]
    );
    let outsider = block_on(teams.show(&email("cy@example.com"), &team("design")));
    assert_eq!(outsider.err(), Some(TeamsError::NotFound));
    let missing = block_on(teams.show(&email("ana@example.com"), &team("legal")));
    assert_eq!(missing.err(), Some(TeamsError::NotFound));
}

#[test]
fn the_reads_agree_with_the_document() {
    let (teams, _) = loaded(&design());
    assert!(!teams.manages());
    assert_eq!(
        block_on(teams.teams_of(&email("ana@example.com"))).unwrap(),
        vec![team("design"), team("ops")]
    );
    assert!(block_on(teams.exists(&team("sales"))).unwrap());
    assert!(!block_on(teams.exists(&team("legal"))).unwrap());
    assert_eq!(
        block_on(teams.members(&team("legal"))).err(),
        Some(TeamsError::NotFound)
    );
}

#[test]
fn every_change_answers_managed_elsewhere_and_the_teams_stay_as_the_document_says() {
    let (teams, _) = loaded(&design());
    let ana = email("ana@example.com");
    let dee = email("dee@example.com");
    let design = team("design");
    assert_eq!(
        block_on(teams.create(&dee, team("legal"), Timestamp::from_secs(1))).err(),
        Some(TeamsError::ManagedElsewhere)
    );
    assert_eq!(
        block_on(teams.add(&ana, &design, &dee)),
        Err(TeamsError::ManagedElsewhere)
    );
    assert_eq!(
        block_on(teams.remove(&ana, &design, &ana)),
        Err(TeamsError::ManagedElsewhere)
    );
    assert_eq!(
        block_on(teams.transfer(&ana, &design, &dee)).err(),
        Some(TeamsError::ManagedElsewhere)
    );
    assert_eq!(
        block_on(teams.delete(&ana, &design)),
        Err(TeamsError::ManagedElsewhere)
    );
    assert_eq!(
        block_on(teams.finish_delete(&ana, &design)),
        Err(TeamsError::ManagedElsewhere)
    );
    assert_eq!(block_on(teams.members(&design)).unwrap().len(), 2);
    assert!(!block_on(teams.exists(&team("legal"))).unwrap());
}

#[test]
fn a_username_deployment_reads_usernames_and_refuses_an_address() {
    let sync = Sync::holding(&json!({"teams": {"design": ["ana", "bo"]}}));
    let teams = block_on(DocumentTeams::load(sync, IdentityKind::Username)).unwrap();
    let ana = IdentityKind::Username.person("ana").unwrap();
    assert_eq!(
        block_on(teams.teams_of(&ana)).unwrap(),
        vec![team("design")]
    );
    let mixed = Sync::holding(&json!({"teams": {"design": ["ana", "bo@example.com"]}}));
    let refusal = block_on(DocumentTeams::load(mixed, IdentityKind::Username)).unwrap_err();
    assert_eq!(
        refusal,
        DocumentRefused::Member {
            team: team("design"),
            position: 2
        }
    );
    assert!(!refusal.to_string().contains("bo@"), "{refusal}");
}

#[test]
fn a_document_that_is_not_the_teams_shape_is_refused_without_repeating_its_text() {
    assert!(matches!(
        refused(b"{\"teams\": "),
        DocumentRefused::Malformed { .. }
    ));
    let wrong = refused(br#"{"teams": {"design": "ana@example.com"}}"#);
    assert!(matches!(wrong, DocumentRefused::Malformed { .. }));
    assert!(!wrong.to_string().contains("ana@"), "{wrong}");
    let extra = refused(br#"{"teams": {}, "owners": {}}"#);
    assert!(matches!(extra, DocumentRefused::Malformed { .. }));
    assert_eq!(
        refused(br#"{"teams": {"design": ["not an address"]}}"#),
        DocumentRefused::Member {
            team: team("design"),
            position: 1
        }
    );
    assert_eq!(
        refused(br#"{"teams": {"ana@example.com": []}}"#),
        DocumentRefused::TeamName { position: 1 }
    );
}

#[test]
fn a_document_over_its_byte_limit_is_refused() {
    let padding = "x".repeat(MAX_DOCUMENT_BYTES);
    let big = format!("{{\"teams\": {{}}, \"pad\": \"{padding}\"}}");
    assert_eq!(refused(big.as_bytes()), DocumentRefused::TooLarge);
}

#[test]
fn a_document_with_too_many_teams_is_refused_and_one_at_the_limit_is_read() {
    let names = |count: usize| -> Value {
        let teams: serde_json::Map<String, Value> = (0..count)
            .map(|index| (format!("t{index}"), json!(["ana@example.com"])))
            .collect();
        // One person in every team would pass the joined limit, so each
        // team gets a member of its own.
        let teams: serde_json::Map<String, Value> = teams
            .into_iter()
            .enumerate()
            .map(|(index, (name, _))| (name, json!([format!("p{index}@example.com")])))
            .collect();
        json!({ "teams": teams })
    };
    let at_limit = Sync::holding(&names(MAX_DIRECTORY_TEAMS));
    assert!(block_on(DocumentTeams::load(at_limit, IdentityKind::Email)).is_ok());
    let over = serde_json::to_vec(&names(MAX_DIRECTORY_TEAMS + 1)).unwrap();
    assert_eq!(refused(&over), DocumentRefused::TooManyTeams);
}

#[test]
fn a_team_with_too_many_members_is_refused_and_one_at_the_limit_is_read() {
    let members =
        |count: usize| -> Vec<String> { (0..count).map(|i| format!("p{i}@example.com")).collect() };
    let at_limit = Sync::holding(&json!({"teams": {"big": members(MAX_DIRECTORY_MEMBERS)}}));
    assert!(block_on(DocumentTeams::load(at_limit, IdentityKind::Email)).is_ok());
    let over =
        serde_json::to_vec(&json!({"teams": {"big": members(MAX_DIRECTORY_MEMBERS + 1)}})).unwrap();
    assert_eq!(
        refused(&over),
        DocumentRefused::TooManyMembers { team: team("big") }
    );
}

#[test]
fn a_person_in_too_many_teams_is_refused_naming_the_team_and_position_and_never_the_address() {
    let teams = |count: usize| -> Value {
        let teams: serde_json::Map<String, Value> = (0..count)
            .map(|index| {
                (
                    format!("t{index:02}"),
                    json!(["bo@example.com", "ana@example.com"]),
                )
            })
            .collect();
        json!({ "teams": teams })
    };
    let at_limit = Sync::holding(&teams(MAX_TEAMS_JOINED));
    assert!(block_on(DocumentTeams::load(at_limit, IdentityKind::Email)).is_ok());
    let refusal = refused(&serde_json::to_vec(&teams(MAX_TEAMS_JOINED + 1)).unwrap());
    assert_eq!(
        refusal,
        DocumentRefused::TooManyJoined {
            team: team(&format!("t{MAX_TEAMS_JOINED:02}")),
            position: 1
        }
    );
    let text = refusal.to_string();
    assert!(text.contains("t32") && text.contains('1'), "{text}");
    assert!(!text.contains("example.com"), "{text}");
}

#[test]
fn an_unreadable_document_is_refused_at_load() {
    let sync = Sync::default();
    sync.fail("the file does not exist");
    let refusal = block_on(DocumentTeams::load(sync, IdentityKind::Email)).unwrap_err();
    assert_eq!(
        refusal,
        DocumentRefused::Unreadable(DocumentUnreadable::new("the file does not exist"))
    );
}

#[test]
fn a_copy_younger_than_the_max_age_is_kept_and_an_older_one_is_read_again() {
    let (teams, sync) = loaded(&design());
    let clock = Clock::new();
    let teams = teams.with_clock(clock.reader());
    let reads = sync.reads();
    sync.write(&json!({"teams": {"design": ["ana@example.com"]}}));
    clock.advance(DOCUMENT_MAX_AGE - Duration::from_secs(1));
    assert_eq!(block_on(teams.members(&team("design"))).unwrap().len(), 2);
    assert_eq!(sync.reads(), reads);
    clock.advance(Duration::from_secs(1));
    assert_eq!(block_on(teams.members(&team("design"))).unwrap().len(), 1);
    assert_eq!(sync.reads(), reads + 1);
    assert_eq!(block_on(teams.members(&team("design"))).unwrap().len(), 1);
    assert_eq!(sync.reads(), reads + 1);
}

#[test]
fn a_refused_reread_keeps_the_last_good_copy_and_reports_why() {
    let (teams, sync) = loaded(&design());
    let clock = Clock::new();
    let seen: Arc<Mutex<Vec<DocumentRefused>>> = Arc::default();
    let report = Arc::clone(&seen);
    let teams = teams
        .with_clock(clock.reader())
        .on_refused(move |refusal: &DocumentRefused| report.lock().unwrap().push(refusal.clone()));
    sync.write_bytes(b"{".to_vec());
    clock.advance(DOCUMENT_MAX_AGE);
    assert_eq!(block_on(teams.members(&team("design"))).unwrap().len(), 2);
    assert!(matches!(
        seen.lock().unwrap().as_slice(),
        [DocumentRefused::Malformed { .. }]
    ));
    sync.fail("access denied");
    clock.advance(DOCUMENT_MAX_AGE);
    assert_eq!(block_on(teams.members(&team("design"))).unwrap().len(), 2);
    assert_eq!(seen.lock().unwrap().len(), 2);
    sync.write(&json!({"teams": {"design": ["ana@example.com"]}}));
    clock.advance(DOCUMENT_MAX_AGE);
    assert_eq!(block_on(teams.members(&team("design"))).unwrap().len(), 1);
}

#[test]
fn a_team_that_leaves_the_document_stops_existing_and_returns_with_its_name() {
    let (teams, sync) = loaded(&design());
    let clock = Clock::new();
    let teams = teams.with_clock(clock.reader());
    sync.write(&json!({"teams": {"ops": ["ana@example.com"]}}));
    clock.advance(DOCUMENT_MAX_AGE);
    assert!(!block_on(teams.exists(&team("design"))).unwrap());
    assert_eq!(
        block_on(teams.teams_of(&email("bo@example.com"))).unwrap(),
        Vec::<TeamName>::new()
    );
    sync.write(&design());
    clock.advance(DOCUMENT_MAX_AGE);
    assert!(block_on(teams.exists(&team("design"))).unwrap());
}

#[test]
fn the_debug_output_names_no_member() {
    let (teams, _) = loaded(&design());
    let shown = format!("{teams:?}");
    assert!(!shown.contains("example.com"), "{shown}");
}

#[test]
fn a_document_that_names_one_team_twice_is_refused_as_malformed() {
    let twice =
        refused(br#"{"teams": {"design": ["ana@example.com"], "design": ["bo@example.com"]}}"#);
    assert!(
        matches!(twice, DocumentRefused::Malformed { .. }),
        "{twice:?}"
    );
    assert!(!twice.to_string().contains("example.com"), "{twice}");
}

/// Teams over a [`Slow`] document holding [`design`], aged by a clock that
/// a test moves.
fn slow() -> (
    DocumentTeams<Slow, impl Fn() -> Instant + Send + std::marker::Sync>,
    Slow,
    Clock,
) {
    let slow = Slow::default();
    slow.sync.write(&design());
    let clock = Clock::new();
    let teams = block_on(DocumentTeams::load(slow.clone(), IdentityKind::Email))
        .unwrap()
        .with_clock(clock.reader());
    (teams, slow, clock)
}

#[test]
fn a_second_stale_call_during_a_slow_read_answers_from_the_old_copy_without_reading() {
    let (teams, slow, clock) = slow();
    let reads = slow.sync.reads();
    slow.sync
        .write(&json!({"teams": {"design": ["ana@example.com"]}}));
    slow.shut();
    clock.advance(DOCUMENT_MAX_AGE);
    let design = team("design");
    let mut first = pin!(teams.members(&design));
    let mut context = Context::from_waker(Waker::noop());
    assert!(first.as_mut().poll(&mut context).is_pending());
    assert_eq!(slow.sync.reads(), reads + 1);
    assert_eq!(block_on(teams.members(&design)).unwrap().len(), 2);
    assert_eq!(block_on(teams.members(&design)).unwrap().len(), 2);
    assert_eq!(slow.sync.reads(), reads + 1);
    slow.open();
    let Poll::Ready(fresh) = first.as_mut().poll(&mut context) else {
        panic!("the read did not finish once the gate opened");
    };
    assert_eq!(fresh.unwrap().len(), 1);
    assert_eq!(block_on(teams.members(&design)).unwrap().len(), 1);
    assert_eq!(slow.sync.reads(), reads + 1);
}

#[test]
fn a_read_whose_call_is_dropped_clears_the_reading_mark_and_the_next_call_reads() {
    let (teams, slow, clock) = slow();
    let reads = slow.sync.reads();
    slow.sync
        .write(&json!({"teams": {"design": ["ana@example.com"]}}));
    slow.shut();
    clock.advance(DOCUMENT_MAX_AGE);
    let design = team("design");
    let mut first = Box::pin(teams.members(&design));
    let mut context = Context::from_waker(Waker::noop());
    assert!(first.as_mut().poll(&mut context).is_pending());
    drop(first);
    slow.open();
    assert_eq!(block_on(teams.members(&design)).unwrap().len(), 1);
    assert_eq!(slow.sync.reads(), reads + 2);
}

//! Plan tests of `Flares::search` over the in-memory stores: the ten
//! examples of docs/search.md against shares owned by three people, and a
//! property test that no query finds a share the caller cannot open.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemoryShareStore,
    MemoryTeamStore,
};
use flare_core::{
    Admission, AllAdmitted, DocumentText, FlareTeams, Flares, Group, MailDomain, NamedRecipients,
    NewFlare, Person, Principal, PrincipalId, Role, SearchHit, ShareContent, ShareId, ShareTitle,
    Standing, TeamName, Teams, Timestamp,
};
use flare_query::parse;
use proptest::prelude::*;

type Memory = Flares<
    MemoryShareStore,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
>;

/// [`Memory`] with teams.
type TeamMemory = Flares<
    MemoryShareStore,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
    FlareTeams<MemoryTeamStore>,
>;

fn email(text: &str) -> Person {
    Person::try_from(text).unwrap()
}

fn person(address: &str) -> Principal {
    Principal::with_person(
        PrincipalId::try_from(address).unwrap(),
        email(address),
        None,
    )
}

const ME: &str = "mei@example.org";
const ANA: &str = "ana@example.org";
const JO: &str = "jo@example.org";

/// The recipients directory of the fixture: ana and jo by name.
fn directory() -> NamedRecipients {
    NamedRecipients::new([("ana".to_owned(), email(ANA)), ("jo".to_owned(), email(JO))])
}

fn id(byte: u8) -> ShareId {
    ShareId::from_random([byte; 16])
}

/// Share `byte`, owned by `owner`, titled `title`, created at `byte` × 100
/// seconds and shared with `with`.
fn share(flares: &Memory, byte: u8, owner: &str, title: &str, with: &[&str]) {
    let flare = NewFlare::new(
        id(byte),
        ShareTitle::try_from(title).unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        with.iter().map(|text| email(text)).collect(),
        Vec::new(),
    );
    let at = Timestamp::from_secs(u64::from(byte) * 100);
    block_on(flares.share(&person(owner), flare, at)).unwrap();
}

/// Eight shares: three of mei's, five of ana's and jo's, four of which are
/// granted to mei. Share 7 is ana's and is not granted to mei. Mei is an
/// admin of share 6.
fn fixture() -> Memory {
    let flares = Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::new(),
        MemoryCommentStore::new(),
        MemoryCursorStore::new(),
        AllAdmitted,
    );
    share(&flares, 1, ME, "Budget review 2027", &[]);
    share(&flares, 2, ME, "Team notes", &[ANA]);
    share(&flares, 3, ANA, "Budget review notes", &[ME]);
    share(&flares, 4, ANA, "Draft notes", &[ME]);
    share(&flares, 5, JO, "Q3 budget", &[ME, ANA]);
    share(&flares, 6, JO, "Launch review, first draft", &[]);
    share(&flares, 7, ANA, "Secret budget", &[JO]);
    share(&flares, 8, ME, "Review: first draft of the plan", &[]);
    let at = Timestamp::from_secs(900);
    block_on(
        flares
            .shares()
            .grant(&person(JO), &id(6), &email(ME), Role::Admin, at),
    )
    .unwrap();
    flares
}

fn search(flares: &Memory, who: &str, text: &str) -> Vec<SearchHit> {
    let query = parse(text).unwrap();
    block_on(flares.search(&person(who), &query, &directory(), false, 50))
        .unwrap()
        .hits
}

fn numbers(hits: &[SearchHit]) -> Vec<u8> {
    hits.iter()
        .map(|hit| (1..=8).find(|byte| hit.share.id() == &id(*byte)).unwrap())
        .collect()
}

#[test]
fn each_documented_example_returns_its_shares_newest_first() {
    let flares = fixture();
    let examples: [(&str, &[u8]); 10] = [
        ("budget", &[5, 3, 1]),
        ("\"budget review\"", &[3, 1]),
        ("review draft", &[8, 6]),
        ("from:me", &[8, 2, 1]),
        ("from:ana@example.org", &[4, 3]),
        ("from:ana,jo", &[6, 5, 4, 3]),
        ("-from:me", &[6, 5, 4, 3]),
        ("notes -draft", &[3, 2]),
        ("type:markdown", &[8, 6, 5, 4, 3, 2, 1]),
        ("review from:me -\"first draft\"", &[1]),
    ];
    for (text, expected) in examples {
        assert_eq!(numbers(&search(&flares, ME, text)), expected, "{text}");
    }
}

#[test]
fn a_hit_carries_what_the_caller_is_to_the_share() {
    let flares = fixture();
    let hits = search(&flares, ME, "review");
    let standings: Vec<Standing> = hits.iter().map(|hit| hit.standing).collect();
    assert_eq!(numbers(&hits), [8, 6, 3, 1]);
    assert_eq!(
        standings,
        [
            Standing::Owner,
            Standing::Admin,
            Standing::Reader,
            Standing::Owner
        ]
    );
}

#[test]
fn a_share_granted_to_others_is_never_found_even_by_its_exact_title() {
    let flares = fixture();
    assert!(search(&flares, ME, "\"Secret budget\"").is_empty());
    assert_eq!(numbers(&search(&flares, JO, "\"Secret budget\"")), [7]);
}

/// Mei is in ana's design team. Ana shares one flare with the team, one
/// with mei directly and one with everyone at example.org.
fn team_direct_and_everyone() -> TeamMemory {
    let flares = Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::new(),
        MemoryCommentStore::new(),
        MemoryCursorStore::new(),
        AllAdmitted,
    )
    .with_teams(FlareTeams::new(MemoryTeamStore::new()))
    .admitting(
        Admission::Domain(MailDomain::try_from("example.org").unwrap()),
        false,
    );
    let design = TeamName::try_from("design").unwrap();
    block_on(flares.create_team(&person(ANA), design.clone(), Timestamp::from_secs(1))).unwrap();
    block_on(
        flares
            .shares()
            .teams()
            .add(&email(ANA), &design, &email(ME)),
    )
    .unwrap();
    let new = |byte: u8, title: &str, with: Vec<Person>, groups: Vec<Group>| {
        NewFlare::new(
            id(byte),
            ShareTitle::try_from(title).unwrap(),
            ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
            with,
            Vec::new(),
        )
        .with_groups(groups)
    };
    let shares = [
        new(
            1,
            "Budget for the team",
            Vec::new(),
            vec![Group::Team(design)],
        ),
        new(2, "Budget for everyone", Vec::new(), vec![Group::Everyone]),
        new(3, "Budget for mei", vec![email(ME)], Vec::new()),
    ];
    for (second, flare) in (200..).zip(shares) {
        block_on(flares.share(&person(ANA), flare, Timestamp::from_secs(second))).unwrap();
    }
    flares
}

fn found(flares: &TeamMemory, everyone: bool) -> Vec<u8> {
    let query = parse("budget").unwrap();
    let found = block_on(flares.search(&person(ME), &query, &directory(), everyone, 50)).unwrap();
    numbers(&found.hits)
}

#[test]
fn a_search_leaves_out_a_share_open_only_to_everyone_by_default() {
    let flares = team_direct_and_everyone();
    assert_eq!(found(&flares, false), [3, 1]);
}

#[test]
fn a_search_with_everyone_finds_a_share_open_to_everyone_signed_in() {
    let flares = team_direct_and_everyone();
    assert_eq!(found(&flares, true), [3, 2, 1]);
}

#[test]
fn a_share_granted_to_the_person_or_their_team_is_found_with_or_without_everyone() {
    let flares = team_direct_and_everyone();
    for everyone in [false, true] {
        let hits = found(&flares, everyone);
        assert!(hits.contains(&3), "direct, everyone {everyone}: {hits:?}");
        assert!(hits.contains(&1), "team, everyone {everyone}: {hits:?}");
    }
}

/// A part of a generated query: a word from the fixture's titles, a
/// filter, or either excluded.
fn part() -> impl Strategy<Value = String> {
    let term = prop_oneof![
        prop::sample::select(vec![
            "budget", "review", "notes", "draft", "secret", "plan", "q3", "team"
        ])
        .prop_map(str::to_owned),
        prop::sample::select(vec![
            "from:me",
            "from:ana",
            "from:jo",
            "from:ana,jo",
            "type:markdown",
            "type:html",
            "\"first draft\"",
        ])
        .prop_map(str::to_owned),
    ];
    (term, any::<bool>()).prop_map(|(term, minus)| if minus { format!("-{term}") } else { term })
}

proptest! {
    #[test]
    fn no_query_finds_a_share_the_caller_cannot_open(
        parts in prop::collection::vec(part(), 0..5),
        only_minus in any::<bool>(),
    ) {
        let flares = fixture();
        let text = parts
            .iter()
            .map(|part| if only_minus && !part.starts_with('-') { format!("-{part}") } else { part.clone() })
            .collect::<Vec<_>>()
            .join(" ");
        let stranger = "sam@example.org";
        share(&flares, 9, stranger, "Sam's budget review draft notes", &[]);
        for hit in search(&flares, stranger, &text) {
            prop_assert_eq!(hit.share.id(), &id(9), "{}", text);
        }
        for hit in search(&flares, ME, &text) {
            prop_assert!(![7, 9].iter().any(|byte| hit.share.id() == &id(*byte)), "{}", text);
        }
    }
}

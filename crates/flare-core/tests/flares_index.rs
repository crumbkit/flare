//! The activity index that `Flares` writes: the exact entries a link, a
//! grant, a revoke and a leave leave behind, and how `activity` reads them.
#![cfg(test)]

#[path = "support/instrumented_responses.rs"]
mod instrumented_responses;

use flare_core::contract::block_on;
use flare_core::memory::{MemoryGrantStore, MemoryShareStore};
use flare_core::{
    ActivityPage, Creators, DocumentText, Email, FlareError, Flares, MAX_EXAMINED, NewEntry,
    NewFlare, NewLink, Position, Principal, PrincipalId, Reader, ResponseStore, ShareContent,
    ShareError, ShareId, ShareTitle, ShareVersion, Timestamp,
};
use instrumented_responses::Instrumented;
use std::sync::atomic::Ordering;

type Service = Flares<MemoryShareStore, MemoryGrantStore, Instrumented>;

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

fn id(n: u16) -> ShareId {
    let [high, low] = n.to_be_bytes();
    let mut bytes = [0_u8; 16];
    bytes[0] = high;
    bytes[1] = low;
    bytes[15] = 1;
    ShareId::from_random(bytes)
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

fn flare(n: u16, with: &[&str], re: &[u16]) -> NewFlare {
    NewFlare::new(
        id(n),
        ShareTitle::try_from(format!("Flare {n}").as_str()).unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        with.iter().map(|name| email(name)).collect(),
        re.iter().map(|n| id(*n)).collect(),
    )
}

fn service() -> Service {
    Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        Instrumented::default(),
        Creators::Everyone,
    )
}

fn share(flares: &Service, who: &str, n: u16, with: &[&str], re: &[u16]) {
    block_on(flares.share(&person(who), flare(n, with, re), at(1))).unwrap();
}

fn grant(flares: &Service, who: &str, n: u16, with: &[&str]) {
    let with: Vec<Email> = with.iter().map(|name| email(name)).collect();
    block_on(flares.grant(&person(who), &id(n), &with, &[], at(2))).unwrap();
}

fn revoke(flares: &Service, who: &str, n: u16, grantee: &str) -> Result<(), FlareError> {
    block_on(flares.revoke(&person(who), &id(n), &email(grantee)))
}

/// Every index entry as (reader's name, response, flare), sorted.
fn entries(flares: &Service) -> Vec<(String, ShareId, ShareId)> {
    let mut found: Vec<_> = flares
        .responses()
        .inner
        .entries()
        .into_iter()
        .map(|entry| {
            let Reader::Person(reader) = entry.reader;
            let name = reader.as_str().trim_end_matches("@example.com").to_owned();
            (name, entry.response, entry.flare)
        })
        .collect();
    found.sort();
    found
}

/// The expected entries, sorted.
fn expect(list: &[(&str, u16, u16)]) -> Vec<(String, ShareId, ShareId)> {
    let mut wanted: Vec<_> = list
        .iter()
        .map(|(name, response, flare)| ((*name).to_owned(), id(*response), id(*flare)))
        .collect();
    wanted.sort();
    wanted
}

fn activity(flares: &Service, who: &str, flare: u16, after: Position) -> ActivityPage {
    block_on(flares.activity(&person(who), &[id(flare)], after, 50)).unwrap()
}

fn shown(page: &ActivityPage) -> Vec<ShareId> {
    page.items
        .iter()
        .map(|item| item.response.id().clone())
        .collect()
}

#[test]
fn a_link_indexes_the_response_for_each_person_who_can_open_both_and_for_nobody_else() {
    let flares = service();
    share(&flares, "alice", 1, &["bob", "carol"], &[]);
    // Bob's response is granted to dave and, without naming her, to alice.
    share(&flares, "bob", 2, &["dave"], &[1]);
    // Carol can open the flare and not the response; dave can open the
    // response and not the flare. Neither gets an entry.
    assert_eq!(entries(&flares), expect(&[("alice", 2, 1), ("bob", 2, 1)]));
    let link = block_on(flares.responses().of_response(&id(2))).unwrap();
    let mut positions: Vec<Position> = flares
        .responses()
        .inner
        .entries()
        .iter()
        .map(|entry| entry.position)
        .collect();
    positions.dedup();
    assert_eq!(positions.len(), 2);
    assert!(
        positions
            .iter()
            .all(|position| *position > link[0].position)
    );
}

#[test]
fn a_grant_on_a_response_indexes_it_for_the_new_reader_on_each_flare_they_can_open() {
    let flares = service();
    share(&flares, "alice", 1, &["bob", "carol"], &[]);
    share(&flares, "alice", 5, &["bob"], &[]);
    share(&flares, "bob", 2, &[], &[1, 5]);
    let before = expect(&[
        ("alice", 2, 1),
        ("alice", 2, 5),
        ("bob", 2, 1),
        ("bob", 2, 5),
    ]);
    assert_eq!(entries(&flares), before);
    // Carol can open flare 1 and not flare 5.
    grant(&flares, "bob", 2, &["carol"]);
    let mut with_carol = before.clone();
    with_carol.extend(expect(&[("carol", 2, 1)]));
    with_carol.sort();
    assert_eq!(entries(&flares), with_carol);
    // Dave can open neither flare.
    grant(&flares, "bob", 2, &["dave"]);
    assert_eq!(entries(&flares), with_carol);
}

#[test]
fn a_grant_on_a_flare_indexes_the_responses_the_new_reader_already_holds_or_owns() {
    let flares = service();
    share(&flares, "alice", 1, &["bob"], &[]);
    share(&flares, "bob", 2, &["dave"], &[1]);
    share(&flares, "bob", 3, &[], &[1]);
    // Carol owns share 4, whose link to flare 1 was written without its
    // index entries, as when an index write failed.
    share(&flares, "carol", 4, &[], &[]);
    let link = NewLink::new(id(4), id(1), ShareVersion::first(), at(1));
    block_on(flares.responses().link(link)).unwrap();
    let before = expect(&[
        ("alice", 2, 1),
        ("bob", 2, 1),
        ("alice", 3, 1),
        ("bob", 3, 1),
    ]);
    assert_eq!(entries(&flares), before);
    // Dave holds response 2 and not response 3.
    grant(&flares, "alice", 1, &["dave"]);
    let mut with_dave = before.clone();
    with_dave.extend(expect(&[("dave", 2, 1)]));
    with_dave.sort();
    assert_eq!(entries(&flares), with_dave);
    // Carol owns response 4, so a grant of flare 1 indexes it for her.
    grant(&flares, "alice", 1, &["carol"]);
    let mut with_carol = with_dave.clone();
    with_carol.extend(expect(&[("carol", 4, 1)]));
    with_carol.sort();
    assert_eq!(entries(&flares), with_carol);
}

#[test]
fn a_revoke_or_a_leave_of_a_response_removes_only_that_readers_entries_for_it() {
    let flares = service();
    share(&flares, "alice", 1, &["bob", "carol", "dave"], &[]);
    share(&flares, "bob", 2, &["carol", "dave"], &[1]);
    share(&flares, "bob", 3, &["carol"], &[1]);
    assert_eq!(
        entries(&flares),
        expect(&[
            ("alice", 2, 1),
            ("bob", 2, 1),
            ("carol", 2, 1),
            ("dave", 2, 1),
            ("alice", 3, 1),
            ("bob", 3, 1),
            ("carol", 3, 1),
        ])
    );
    revoke(&flares, "bob", 2, "carol").unwrap();
    revoke(&flares, "dave", 2, "dave").unwrap();
    assert_eq!(
        entries(&flares),
        expect(&[
            ("alice", 2, 1),
            ("bob", 2, 1),
            ("alice", 3, 1),
            ("bob", 3, 1),
            ("carol", 3, 1),
        ])
    );
}

#[test]
fn a_revoke_of_a_flare_keeps_the_entries_and_activity_refuses_the_flare_until_a_new_grant() {
    let flares = service();
    share(&flares, "alice", 1, &["bob"], &[]);
    share(&flares, "bob", 2, &[], &[1]);
    let both = expect(&[("alice", 2, 1), ("bob", 2, 1)]);
    revoke(&flares, "alice", 1, "bob").unwrap();
    assert_eq!(entries(&flares), both);
    let refused = block_on(flares.activity(&person("bob"), &[id(1)], Position::new(0), 50));
    assert_eq!(refused.unwrap_err(), FlareError::NotFound);
    grant(&flares, "alice", 1, &["bob"]);
    assert_eq!(entries(&flares), both);
    assert_eq!(
        shown(&activity(&flares, "bob", 1, Position::new(0))),
        vec![id(2)]
    );
}

#[test]
fn activity_examines_at_most_limit_plus_one_entries_when_thousands_of_hidden_responses_precede_a_visible_one()
 {
    let flares = service();
    let responders: Vec<String> = (0..10).map(|n| format!("p{n}")).collect();
    let mut with: Vec<&str> = responders.iter().map(String::as_str).collect();
    with.push("carol");
    share(&flares, "alice", 1, &with, &[]);
    // 2,000 responses that carol cannot open, then one she can.
    for (n, responder) in (100..2_100).zip(responders.iter().cycle()) {
        share(&flares, responder, n, &[], &[1]);
    }
    share(&flares, "p0", 3_000, &["carol"], &[1]);
    flares.responses().take_examined();
    let page = block_on(flares.activity(&person("carol"), &[id(1)], Position::new(0), 1)).unwrap();
    assert_eq!(shown(&page), vec![id(3_000)]);
    assert!(!page.more);
    assert!(flares.responses().take_examined() <= 2);
    // The admitted twin: alice, the owner, sees the first response at once.
    let owner = block_on(flares.activity(&person("alice"), &[id(1)], Position::new(0), 1)).unwrap();
    assert_eq!(shown(&owner), vec![id(100)]);
    assert!(owner.more);
}

#[test]
fn a_response_granted_after_the_cursor_shows_next_and_a_revoke_hides_it_even_when_the_unindex_fails()
 {
    let flares = service();
    share(&flares, "alice", 1, &["bob", "carol"], &[]);
    share(&flares, "bob", 2, &[], &[1]);
    let first = activity(&flares, "carol", 1, Position::new(0));
    assert!(first.items.is_empty());
    grant(&flares, "bob", 2, &["carol"]);
    let next = activity(&flares, "carol", 1, first.last);
    assert_eq!(shown(&next), vec![id(2)]);
    flares
        .responses()
        .fail_unindex
        .store(true, Ordering::SeqCst);
    let failed = revoke(&flares, "bob", 2, "carol");
    assert_eq!(failed.unwrap_err(), FlareError::ResponsesUnavailable);
    assert!(entries(&flares).contains(&expect(&[("carol", 2, 1)])[0]));
    assert!(
        activity(&flares, "carol", 1, Position::new(0))
            .items
            .is_empty()
    );
    // A repeated revoke finds no grant and still removes the stale entry.
    flares
        .responses()
        .fail_unindex
        .store(false, Ordering::SeqCst);
    let repeated = revoke(&flares, "bob", 2, "carol");
    assert_eq!(
        repeated.unwrap_err(),
        FlareError::Share(ShareError::GrantNotFound)
    );
    assert_eq!(entries(&flares), expect(&[("alice", 2, 1), ("bob", 2, 1)]));
}

#[test]
fn a_repeated_share_grant_writes_the_entries_an_index_failure_left_out() {
    let flares = service();
    share(&flares, "alice", 1, &["bob", "carol"], &[]);
    flares.responses().fail_index.store(true, Ordering::SeqCst);
    let partly = block_on(flares.share(&person("bob"), flare(2, &[], &[1]), at(3)));
    assert!(matches!(
        partly.unwrap_err(),
        FlareError::PartlyShared { .. }
    ));
    assert!(entries(&flares).is_empty());
    flares.responses().fail_index.store(false, Ordering::SeqCst);
    block_on(flares.grant(&person("bob"), &id(2), &[], &[id(1)], at(4))).unwrap();
    assert_eq!(entries(&flares), expect(&[("alice", 2, 1), ("bob", 2, 1)]));
    // A grant whose index fails is repaired by granting the same person
    // again, though they already hold the grant.
    flares.responses().fail_index.store(true, Ordering::SeqCst);
    let failed = block_on(flares.grant(&person("bob"), &id(2), &[email("carol")], &[], at(5)));
    assert!(failed.is_err());
    flares.responses().fail_index.store(false, Ordering::SeqCst);
    grant(&flares, "bob", 2, &["carol"]);
    assert_eq!(
        entries(&flares),
        expect(&[("alice", 2, 1), ("bob", 2, 1), ("carol", 2, 1)])
    );
}

#[test]
fn a_grant_racing_a_link_leaves_an_entry() {
    let flares = service();
    share(&flares, "alice", 1, &["bob", "carol"], &[]);
    for n in 2..40 {
        share(&flares, "bob", n, &[], &[]);
        std::thread::scope(|scope| {
            scope.spawn(|| grant(&flares, "bob", n, &["carol"]));
            scope.spawn(|| {
                block_on(flares.grant(&person("bob"), &id(n), &[], &[id(1)], at(3))).unwrap();
            });
        });
        assert!(
            entries(&flares).contains(&expect(&[("carol", n, 1)])[0]),
            "round {n}"
        );
    }
}

#[test]
fn activity_examines_at_most_1000_entries_a_call_and_says_more_when_stale_entries_fill_them() {
    let flares = service();
    share(&flares, "alice", 1, &["carol"], &[]);
    // 1,200 stale entries for carol: responses that do not exist.
    let carol = Reader::Person(PrincipalId::try_from("carol@example.com").unwrap());
    for n in 100..1_300 {
        let link = NewLink::new(id(n), id(1), ShareVersion::first(), at(1));
        let link = block_on(flares.responses().link(link)).unwrap();
        block_on(
            flares
                .responses()
                .index(vec![NewEntry::new(carol.clone(), link)]),
        )
        .unwrap();
    }
    share(&flares, "alice", 2, &["carol"], &[1]);
    flares.responses().take_examined();
    let first = activity(&flares, "carol", 1, Position::new(0));
    assert!(first.items.is_empty());
    assert!(first.more);
    // The call opens at most 1,000 entries; its last read of 51 (the
    // limit and one more) may hold entries it leaves for the next call.
    assert!(flares.responses().take_examined() <= MAX_EXAMINED + 51);
    let second = activity(&flares, "carol", 1, first.last);
    assert_eq!(shown(&second), vec![id(2)]);
    assert!(!second.more);
}

#[test]
fn a_new_grant_after_a_revoke_whose_unindex_failed_shows_the_response_as_new() {
    let flares = service();
    share(&flares, "alice", 1, &["bob", "carol"], &[]);
    share(&flares, "bob", 2, &["carol"], &[1]);
    flares
        .responses()
        .fail_unindex
        .store(true, Ordering::SeqCst);
    let failed = revoke(&flares, "bob", 2, "carol");
    assert_eq!(failed.unwrap_err(), FlareError::ResponsesUnavailable);
    flares
        .responses()
        .fail_unindex
        .store(false, Ordering::SeqCst);
    // Carol's stale entry stays; her cursor moves past it.
    let cursor = activity(&flares, "carol", 1, Position::new(0));
    assert!(cursor.items.is_empty());
    grant(&flares, "bob", 2, &["carol"]);
    let next = activity(&flares, "carol", 1, cursor.last);
    assert_eq!(shown(&next), vec![id(2)]);
    // She holds one entry for the response, at its new position.
    let carol: Vec<_> = flares
        .responses()
        .inner
        .entries()
        .into_iter()
        .filter(|entry| {
            entry.reader == Reader::Person(PrincipalId::try_from("carol@example.com").unwrap())
        })
        .collect();
    assert_eq!(carol.len(), 1);
    assert!(carol[0].position > cursor.last);
}

#[test]
fn a_repeated_grant_to_a_person_who_holds_the_grant_keeps_their_entry_position() {
    let flares = service();
    share(&flares, "alice", 1, &["bob", "carol"], &[]);
    share(&flares, "bob", 2, &["carol"], &[1]);
    let before = flares.responses().inner.entries();
    grant(&flares, "bob", 2, &["carol"]);
    assert_eq!(flares.responses().inner.entries(), before);
}

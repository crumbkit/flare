//! The `Flares` service over the in-memory stores: who may share, what a
//! response links, and what `activity` shows to whom.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemorySequence,
    MemoryShareStore,
};
use flare_core::{
    ActivityPage, AllAdmitted, CreatorList, DocumentText, Email, FileId, FlareError, Flares,
    ImagePath, MAX_GRANTS, MediaType, NewFlare, NoTeams, Position, Principal, PrincipalId, Role,
    ShareContent, ShareError, ShareId, ShareTitle, ShareVersion, Timestamp, VersionImage,
    VersionImages,
};

type Memory<K = CreatorList> = Flares<
    MemoryShareStore,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
    NoTeams,
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
}

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

fn carol() -> Principal {
    person("carol@example.com")
}

fn outsider() -> Principal {
    person("ola@outside.example")
}

fn id(byte: u8) -> ShareId {
    ShareId::from_random([byte; 16])
}

fn flare(byte: u8, with: &[&str], re: &[ShareId]) -> NewFlare {
    NewFlare::new(
        id(byte),
        ShareTitle::try_from(format!("Flare {byte}").as_str()).unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        with.iter().map(|text| email(text)).collect(),
        re.to_vec(),
    )
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

/// A service whose creators are alice, bob and carol.
fn memory() -> Memory {
    let creators = CreatorList::only(vec![
        email("alice@example.com"),
        email("bob@example.com"),
        email("carol@example.com"),
    ]);
    stores(creators)
}

/// A service in which alice shared flare 1 with bob.
fn with_flare() -> Memory {
    let flares = memory();
    block_on(flares.share(&alice(), flare(1, &["bob@example.com"], &[]), at(1))).unwrap();
    flares
}

fn activity(flares: &Memory, who: &Principal, after: u64) -> Result<ActivityPage, FlareError> {
    block_on(flares.activity(who, &[id(1)], Position::new(after), 50))
}

#[test]
fn a_creator_shares_a_flare_and_each_recipient_can_open_it() {
    let flares = memory();
    let shared = block_on(flares.share(
        &alice(),
        flare(1, &["bob@example.com", "carol@example.com"], &[]),
        at(1),
    ))
    .unwrap();
    assert_eq!(shared.share.id(), &id(1));
    assert_eq!(shared.granted.len(), 2);
    for reader in [bob(), carol()] {
        let opened = block_on(flares.shares().open(&reader, &id(1), None)).unwrap();
        assert_eq!(opened.share.id(), &id(1));
    }
}

#[test]
fn a_person_off_the_creators_list_cannot_share_and_one_on_it_can() {
    let flares = memory();
    let refused = block_on(flares.share(&outsider(), flare(1, &[], &[]), at(1)));
    assert_eq!(refused.unwrap_err(), FlareError::NotACreator);
    assert!(block_on(flares.share(&alice(), flare(1, &[], &[]), at(1))).is_ok());
}

#[test]
fn a_person_off_the_creators_list_cannot_respond_to_a_flare_they_can_open_and_a_creator_can() {
    let flares = with_flare();
    block_on(flares.shares().grant(
        &alice(),
        &id(1),
        &email("ola@outside.example"),
        Role::Reader,
        at(2),
    ))
    .unwrap();
    assert!(block_on(flares.shares().open(&outsider(), &id(1), None)).is_ok());
    let refused = block_on(flares.share(&outsider(), flare(2, &[], &[id(1)]), at(3)));
    assert_eq!(refused.unwrap_err(), FlareError::NotACreator);
    assert!(block_on(flares.share(&bob(), flare(2, &[], &[id(1)]), at(3))).is_ok());
}

#[test]
fn everyone_who_signs_in_may_create_when_creators_is_everyone() {
    let flares = stores(AllAdmitted);
    assert!(block_on(flares.share(&outsider(), flare(1, &[], &[]), at(1))).is_ok());
}

#[test]
fn a_missing_flare_and_a_refused_flare_give_the_same_answer_to_re_activity_and_open() {
    let flares = with_flare();
    // Carol holds no grant on flare 1; flare 9 does not exist.
    let refused = block_on(flares.share(&carol(), flare(2, &[], &[id(1)]), at(3))).unwrap_err();
    let missing = block_on(flares.share(&carol(), flare(3, &[], &[id(9)]), at(3))).unwrap_err();
    assert_eq!(refused, missing);
    assert_eq!(refused, FlareError::NotFound);
    let refused = block_on(flares.activity(&carol(), &[id(1)], Position::new(0), 10));
    let missing = block_on(flares.activity(&carol(), &[id(9)], Position::new(0), 10));
    assert_eq!(refused.unwrap_err(), missing.unwrap_err());
    let refused = block_on(flares.shares().open(&carol(), &id(1), None)).unwrap_err();
    let missing = block_on(flares.shares().open(&carol(), &id(9), None)).unwrap_err();
    assert_eq!(
        (refused, missing),
        (ShareError::NotFound, ShareError::NotFound)
    );
    // The admitted twin: bob can respond to flare 1 and read its activity.
    assert!(block_on(flares.share(&bob(), flare(4, &[], &[id(1)]), at(3))).is_ok());
    assert!(activity(&flares, &bob(), 0).is_ok());
}

#[test]
fn a_refused_re_writes_nothing() {
    let flares = with_flare();
    let refused = block_on(flares.share(&carol(), flare(2, &["bob@example.com"], &[id(1)]), at(3)));
    assert!(refused.is_err());
    let opened = block_on(flares.shares().open(&carol(), &id(2), None));
    assert_eq!(opened.unwrap_err(), ShareError::NotFound);
    assert!(
        block_on(flares.shares().mine(&carol()))
            .unwrap()
            .items()
            .is_empty()
    );
}

#[test]
fn a_response_lists_under_its_flare_with_the_version_it_responds_to() {
    let flares = with_flare();
    let content = ShareContent::Document(DocumentText::try_from("# Two\n").unwrap());
    block_on(flares.shares().publish(&alice(), &id(1), content, at(2))).unwrap();
    let shared =
        block_on(flares.share(&bob(), flare(2, &["alice@example.com"], &[id(1)]), at(3))).unwrap();
    assert_eq!(shared.responds_to, vec![id(1)]);
    let page = activity(&flares, &alice(), 0).unwrap();
    assert_eq!(page.items.len(), 1);
    let item = page.items[0].as_response().unwrap();
    assert_eq!(item.flare, id(1));
    assert_eq!(item.response.id(), &id(2));
    assert_eq!(item.response.owner().as_str(), "bob@example.com");
    assert_eq!(item.version, ShareVersion::try_from(2).unwrap());
    assert_eq!(item.at, at(3));
    assert!(!page.more);
}

#[test]
fn activity_omits_a_response_the_caller_cannot_open_even_when_they_can_open_the_flare() {
    let flares = with_flare();
    block_on(flares.grant(&alice(), &id(1), &[email("carol@example.com")], &[], at(2))).unwrap();
    // Bob responds and shares the response with alice only.
    block_on(flares.share(&bob(), flare(2, &["alice@example.com"], &[id(1)]), at(3))).unwrap();
    let hidden = activity(&flares, &carol(), 0).unwrap();
    assert!(hidden.items.is_empty());
    // The admitted twin: once carol may open the response, she sees it.
    block_on(flares.grant(&bob(), &id(2), &[email("carol@example.com")], &[], at(4))).unwrap();
    let shown = activity(&flares, &carol(), 0).unwrap();
    assert_eq!(shown.items.len(), 1);
}

fn grants_on(flares: &Memory, who: &Principal, share: u8) -> Vec<(String, Role)> {
    let mut grants: Vec<_> = block_on(flares.shares().grants(who, &id(share)))
        .unwrap()
        .iter()
        .map(|grant| (grant.grantee().as_str().to_owned(), grant.role()))
        .collect();
    grants.sort_by(|a, b| a.0.cmp(&b.0));
    grants
}

#[test]
fn a_response_is_shared_with_the_owner_of_the_flare_without_naming_them() {
    let flares = with_flare();
    block_on(flares.share(&bob(), flare(2, &[], &[id(1)]), at(3))).unwrap();
    assert!(block_on(flares.shares().open(&alice(), &id(2), None)).is_ok());
    let with_me = block_on(flares.shares().shared_with_me(&alice(), false)).unwrap();
    assert_eq!(with_me.items().len(), 1);
    assert_eq!(activity(&flares, &alice(), 0).unwrap().items.len(), 1);
}

#[test]
fn granting_a_response_to_a_flare_shares_it_with_the_flare_owner() {
    let flares = with_flare();
    block_on(flares.share(&bob(), flare(2, &[], &[]), at(2))).unwrap();
    assert!(block_on(flares.shares().open(&alice(), &id(2), None)).is_err());
    block_on(flares.grant(&bob(), &id(2), &[], &[id(1)], at(3))).unwrap();
    assert!(block_on(flares.shares().open(&alice(), &id(2), None)).is_ok());
    assert_eq!(activity(&flares, &alice(), 0).unwrap().items.len(), 1);
}

#[test]
fn a_response_to_flares_with_two_owners_grants_each_once_and_one_owner_once() {
    let flares = with_flare();
    block_on(flares.share(&carol(), flare(5, &["bob@example.com"], &[]), at(2))).unwrap();
    block_on(flares.share(&alice(), flare(6, &["bob@example.com"], &[]), at(2))).unwrap();
    block_on(flares.share(&bob(), flare(2, &[], &[id(1), id(5), id(6)]), at(3))).unwrap();
    let readers = vec![
        ("alice@example.com".to_owned(), Role::Reader),
        ("carol@example.com".to_owned(), Role::Reader),
    ];
    assert_eq!(grants_on(&flares, &bob(), 2), readers);
}

#[test]
fn an_owner_named_or_already_admin_keeps_one_grant_at_their_role() {
    let flares = with_flare();
    block_on(flares.share(&bob(), flare(2, &["alice@example.com"], &[id(1)]), at(3))).unwrap();
    assert_eq!(
        grants_on(&flares, &bob(), 2),
        vec![("alice@example.com".to_owned(), Role::Reader)]
    );
    block_on(flares.share(&bob(), flare(3, &[], &[]), at(4))).unwrap();
    block_on(flares.shares().grant(
        &bob(),
        &id(3),
        &email("alice@example.com"),
        Role::Admin,
        at(5),
    ))
    .unwrap();
    block_on(flares.grant(&bob(), &id(3), &[], &[id(1)], at(6))).unwrap();
    assert_eq!(
        grants_on(&flares, &bob(), 3),
        vec![("alice@example.com".to_owned(), Role::Admin)]
    );
}

#[test]
fn the_owner_responding_to_their_own_flare_gets_no_extra_grant() {
    let flares = with_flare();
    block_on(flares.share(&alice(), flare(2, &[], &[id(1)]), at(3))).unwrap();
    assert!(grants_on(&flares, &alice(), 2).is_empty());
}

#[test]
fn a_refused_owner_grant_leaves_the_response_unlinked_and_a_repeat_finishes_it() {
    let flares = with_flare();
    let with: Vec<String> = (0..MAX_GRANTS)
        .map(|n| format!("p{n}@example.com"))
        .collect();
    let with: Vec<&str> = with.iter().map(String::as_str).collect();
    let refused = block_on(flares.share(&bob(), flare(2, &with, &[id(1)]), at(3))).unwrap_err();
    assert_eq!(
        refused,
        FlareError::PartlyShared {
            share: id(2),
            granted: MAX_GRANTS,
            cause: ShareError::TooManyGrants,
        }
    );
    assert!(activity(&flares, &alice(), 0).unwrap().items.is_empty());
    block_on(
        flares
            .shares()
            .revoke(&bob(), &id(2), &email("p0@example.com")),
    )
    .unwrap();
    block_on(flares.grant(&bob(), &id(2), &[], &[id(1)], at(4))).unwrap();
    assert_eq!(activity(&flares, &alice(), 0).unwrap().items.len(), 1);
}

#[test]
fn activity_reports_no_count_of_hidden_responses_and_moves_its_cursor_as_it_would_without_them() {
    let flares = with_flare();
    block_on(flares.grant(&alice(), &id(1), &[email("carol@example.com")], &[], at(2))).unwrap();
    let before = activity(&flares, &carol(), 0).unwrap();
    block_on(flares.share(&bob(), flare(2, &[], &[id(1)]), at(3))).unwrap();
    block_on(flares.share(&carol(), flare(3, &[], &[]), at(4))).unwrap();
    let after = activity(&flares, &carol(), before.last.get()).unwrap();
    assert!(after.items.is_empty());
    assert!(!after.more);
    // The cursor is the store's head, which any share anywhere moves, so
    // it says nothing about this flare.
    assert_eq!(
        after.last,
        block_on(flares.activity(&bob(), &[id(1)], Position::new(0), 50))
            .unwrap()
            .last
    );
}

#[test]
fn a_cursor_returns_only_newer_responses() {
    let flares = with_flare();
    block_on(flares.share(&bob(), flare(2, &["alice@example.com"], &[id(1)]), at(3))).unwrap();
    let first = activity(&flares, &alice(), 0).unwrap();
    assert_eq!(first.items.len(), 1);
    let none = activity(&flares, &alice(), first.last.get()).unwrap();
    assert!(none.items.is_empty());
    block_on(flares.share(&carol(), flare(3, &[], &[]), at(4))).unwrap();
    block_on(flares.grant(&alice(), &id(1), &[email("carol@example.com")], &[], at(5))).unwrap();
    block_on(flares.share(&carol(), flare(4, &["alice@example.com"], &[id(1)]), at(6))).unwrap();
    let newer = activity(&flares, &alice(), first.last.get()).unwrap();
    let ids: Vec<_> = newer
        .items
        .iter()
        .filter_map(|item| item.as_response())
        .map(|item| item.response.id().clone())
        .collect();
    assert_eq!(ids, vec![id(4)]);
}

#[test]
fn each_activity_call_checks_access_again_so_a_cursor_grants_nothing() {
    let flares = with_flare();
    let first = activity(&flares, &bob(), 0).unwrap();
    block_on(
        flares
            .shares()
            .revoke(&alice(), &id(1), &email("bob@example.com")),
    )
    .unwrap();
    let refused = activity(&flares, &bob(), first.last.get());
    assert_eq!(refused.unwrap_err(), FlareError::NotFound);
    // The admitted twin: alice, still the owner, reads with the same cursor.
    assert!(activity(&flares, &alice(), first.last.get()).is_ok());
}

#[test]
fn activity_pages_oldest_first_and_says_more_when_the_limit_cuts_it() {
    let flares = with_flare();
    for byte in 2..=4 {
        block_on(flares.share(&bob(), flare(byte, &["alice@example.com"], &[id(1)]), at(3)))
            .unwrap();
    }
    let page = block_on(flares.activity(&alice(), &[id(1)], Position::new(0), 2)).unwrap();
    let ids: Vec<_> = page
        .items
        .iter()
        .filter_map(|item| item.as_response())
        .map(|item| item.response.id().clone())
        .collect();
    assert_eq!(ids, vec![id(2), id(3)]);
    assert!(page.more);
    let rest = block_on(flares.activity(&alice(), &[id(1)], page.last, 2)).unwrap();
    let ids: Vec<_> = rest
        .items
        .iter()
        .filter_map(|item| item.as_response())
        .map(|item| item.response.id().clone())
        .collect();
    assert_eq!(ids, vec![id(4)]);
    assert!(!rest.more);
}

#[test]
fn a_response_to_two_flares_names_the_other_only_when_the_caller_can_open_it() {
    let flares = with_flare();
    block_on(flares.share(&carol(), flare(5, &["bob@example.com"], &[]), at(2))).unwrap();
    block_on(flares.share(
        &bob(),
        flare(2, &["alice@example.com"], &[id(1), id(5)]),
        at(3),
    ))
    .unwrap();
    let page = activity(&flares, &alice(), 0).unwrap();
    assert!(
        page.items[0].as_response().unwrap().also.is_empty(),
        "alice cannot open flare 5"
    );
    block_on(flares.grant(&carol(), &id(5), &[email("alice@example.com")], &[], at(4))).unwrap();
    let page = activity(&flares, &alice(), 0).unwrap();
    let also: Vec<_> = page.items[0]
        .as_response()
        .unwrap()
        .also
        .iter()
        .map(|share| share.id().clone())
        .collect();
    assert_eq!(also, vec![id(5)]);
}

#[test]
fn a_repeated_re_links_once() {
    let flares = with_flare();
    block_on(flares.share(
        &bob(),
        flare(2, &["alice@example.com"], &[id(1), id(1)]),
        at(3),
    ))
    .unwrap();
    block_on(flares.grant(&bob(), &id(2), &[], &[id(1)], at(4))).unwrap();
    assert_eq!(activity(&flares, &alice(), 0).unwrap().items.len(), 1);
}

#[test]
fn granting_a_flare_does_not_demote_an_admin() {
    let flares = with_flare();
    block_on(flares.shares().grant(
        &alice(),
        &id(1),
        &email("carol@example.com"),
        Role::Admin,
        at(2),
    ))
    .unwrap();
    block_on(flares.grant(&alice(), &id(1), &[email("carol@example.com")], &[], at(3))).unwrap();
    let grants = block_on(flares.shares().grants(&alice(), &id(1))).unwrap();
    let carol = grants
        .iter()
        .find(|grant| grant.grantee().as_str() == "carol@example.com")
        .unwrap();
    assert_eq!(carol.role(), Role::Admin);
}

#[test]
fn only_the_owner_adds_a_flare_to_what_a_share_responds_to_and_a_reader_is_refused() {
    let flares = with_flare();
    block_on(flares.share(&carol(), flare(5, &["bob@example.com"], &[]), at(2))).unwrap();
    // Bob reads flare 5 and cannot make it respond to flare 1.
    let refused = block_on(flares.grant(&bob(), &id(5), &[], &[id(1)], at(3)));
    assert_eq!(
        refused.unwrap_err(),
        FlareError::Share(ShareError::Forbidden)
    );
    block_on(flares.grant(&alice(), &id(1), &[email("carol@example.com")], &[], at(3))).unwrap();
    assert!(block_on(flares.grant(&carol(), &id(5), &[], &[id(1)], at(4))).is_ok());
}

#[test]
fn a_share_whose_grants_stop_partway_names_the_share_and_the_grants_written() {
    let flares = memory();
    let with: Vec<String> = (0..=MAX_GRANTS)
        .map(|n| format!("p{n}@example.com"))
        .collect();
    let with: Vec<&str> = with.iter().map(String::as_str).collect();
    let refused = block_on(flares.share(&alice(), flare(1, &with, &[]), at(1))).unwrap_err();
    assert_eq!(
        refused,
        FlareError::PartlyShared {
            share: id(1),
            granted: MAX_GRANTS,
            cause: ShareError::TooManyGrants,
        }
    );
    // The share exists, so the owner finishes the grants with `grant`.
    assert!(block_on(flares.shares().open(&alice(), &id(1), None)).is_ok());
}

#[test]
fn a_principal_with_no_verified_email_cannot_share() {
    let flares = memory();
    let nobody = Principal::new(PrincipalId::try_from("local:nobody").unwrap());
    let refused = block_on(flares.share(&nobody, flare(1, &[], &[]), at(1)));
    assert_eq!(
        refused.unwrap_err(),
        FlareError::Share(ShareError::NoVerifiedEmail)
    );
}

/// The response ids on each page of activity across `ids`, read `limit`
/// at a time from the start until a page says no more follow.
fn pages(flares: &Memory, who: &Principal, ids: &[ShareId], limit: usize) -> Vec<Vec<ShareId>> {
    let mut pages = Vec::new();
    let mut after = Position::new(0);
    loop {
        let page = block_on(flares.activity(who, ids, after, limit)).unwrap();
        pages.push(
            page.items
                .iter()
                .filter_map(|item| item.as_response())
                .map(|item| item.response.id().clone())
                .collect(),
        );
        after = page.last;
        if !page.more {
            return pages;
        }
        assert!(pages.len() < 10, "activity paging did not end");
    }
}

/// Bob responds to `to` with share `byte`, shared with carol when
/// `visible` and with nobody else otherwise. The flare's owner, alice,
/// always sees it.
fn respond(flares: &Memory, byte: u8, to: ShareId, visible: bool) {
    let with: &[&str] = if visible { &["carol@example.com"] } else { &[] };
    block_on(flares.share(&bob(), flare(byte, with, &[to]), at(3))).unwrap();
}

#[test]
fn activity_across_two_flares_returns_every_visible_response_when_hidden_ones_cut_a_read() {
    let flares = with_flare();
    block_on(flares.share(&alice(), flare(5, &["bob@example.com"], &[]), at(2))).unwrap();
    for flare in [id(1), id(5)] {
        block_on(flares.grant(&alice(), &flare, &[email("carol@example.com")], &[], at(2)))
            .unwrap();
    }
    // Flare 1 holds two hidden responses and then two visible ones, so a
    // read of three links from flare 1 is cut; flare 5's response is newer.
    respond(&flares, 2, id(1), false);
    respond(&flares, 3, id(1), false);
    respond(&flares, 4, id(1), true);
    respond(&flares, 6, id(1), true);
    respond(&flares, 7, id(5), true);
    let pages = pages(&flares, &carol(), &[id(1), id(5)], 2);
    let seen: Vec<ShareId> = pages.concat();
    assert_eq!(seen, vec![id(4), id(6), id(7)]);
    // The admitted twin: bob, who can open every response, sees all five.
    let bob_seen = self::pages(&flares, &bob(), &[id(1), id(5)], 2).concat();
    assert_eq!(bob_seen, vec![id(2), id(3), id(4), id(6), id(7)]);
}

#[test]
fn a_page_that_says_more_follow_is_never_empty_when_hidden_responses_fill_a_read() {
    let flares = with_flare();
    block_on(flares.grant(&alice(), &id(1), &[email("carol@example.com")], &[], at(2))).unwrap();
    for byte in 2..=4 {
        respond(&flares, byte, id(1), false);
    }
    respond(&flares, 5, id(1), true);
    let page = block_on(flares.activity(&carol(), &[id(1)], Position::new(0), 2)).unwrap();
    let ids: Vec<_> = page
        .items
        .iter()
        .filter_map(|item| item.as_response())
        .map(|item| item.response.id().clone())
        .collect();
    assert_eq!(ids, vec![id(5)]);
    assert!(!page.more);
}

#[test]
fn a_grant_whose_first_write_is_a_refused_owner_grant_answers_the_refusal_and_writes_nothing() {
    let flares = with_flare();
    block_on(flares.share(&bob(), flare(2, &[], &[]), at(2))).unwrap();
    for n in 0..MAX_GRANTS {
        let reader = email(&format!("p{n}@example.com"));
        block_on(
            flares
                .shares()
                .grant(&bob(), &id(2), &reader, Role::Reader, at(3)),
        )
        .unwrap();
    }
    let refused = block_on(flares.grant(&bob(), &id(2), &[], &[id(1)], at(4))).unwrap_err();
    assert_eq!(refused, FlareError::Share(ShareError::TooManyGrants));
    assert!(activity(&flares, &alice(), 0).unwrap().items.is_empty());
}

#[test]
fn a_creator_whose_principal_id_differs_from_their_address_gets_no_grant_on_a_response_to_their_own_flare()
 {
    let flares = memory();
    let alice = Principal::with_email(
        PrincipalId::try_from("google:a").unwrap(),
        email("alice@example.com"),
    );
    block_on(flares.share(&alice, flare(1, &[], &[]), at(1))).unwrap();
    block_on(flares.share(&alice, flare(2, &[], &[id(1)]), at(2))).unwrap();
    assert!(grants_on(&flares, &alice, 2).is_empty());
    block_on(flares.share(&alice, flare(3, &[], &[]), at(3))).unwrap();
    block_on(flares.grant(&alice, &id(3), &[], &[id(1)], at(4))).unwrap();
    assert!(grants_on(&flares, &alice, 3).is_empty());
}

#[test]
fn a_share_check_refuses_a_non_creator_and_a_closed_re_flare_and_writes_nothing() {
    let flares = memory();
    block_on(flares.share(&alice(), flare(1, &[], &[]), at(1))).unwrap();
    assert_eq!(
        block_on(flares.may_share(&outsider(), &flare(2, &[], &[]))),
        Err(FlareError::NotACreator)
    );
    assert_eq!(
        block_on(flares.may_share(&bob(), &flare(2, &[], &[id(1)]))),
        Err(FlareError::NotFound)
    );
    assert_eq!(
        block_on(flares.shares().open(&bob(), &id(2), None)).unwrap_err(),
        ShareError::NotFound
    );
    // The admitted twin: alice may respond to her own flare.
    assert_eq!(
        block_on(flares.may_share(&alice(), &flare(2, &[], &[id(1)]))),
        Ok(())
    );
}

#[test]
fn a_flare_shared_with_images_keeps_its_image_table_on_version_one() {
    let flares = memory();
    let table = VersionImages::try_from(vec![VersionImage::new(
        ImagePath::try_from("img/a.png").unwrap(),
        FileId::from_random([4; 16]),
        MediaType::try_from("image/png").unwrap(),
        12,
    )])
    .unwrap();
    let new = flare(1, &[], &[]).with_images(table.clone());
    block_on(flares.share(&alice(), new, at(1))).unwrap();
    let opened = block_on(flares.shares().open(&alice(), &id(1), None)).unwrap();
    assert_eq!(opened.version.images(), &table);
}

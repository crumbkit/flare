//! Comments, replies and cursors through the `Flares` service over the
//! in-memory stores: who may read and write, the reply depth, and how the
//! activity feed orders comment events with response links.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemorySequence,
    MemoryShareStore,
};
use flare_core::{
    ActivityItem, CommentAction, CommentBody, CommentDraft, CommentId, CommentKey, Creators,
    CursorName, DocumentText, Email, FlareError, Flares, NewFlare, Position, Principal,
    PrincipalId, ShareContent, ShareId, ShareTitle, Timestamp,
};

type Memory = Flares<
    MemoryShareStore,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
>;

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

fn outsider() -> Principal {
    person("ola@outside.example")
}

fn flare_id() -> ShareId {
    ShareId::from_random([1; 16])
}

fn comment_id(byte: u8) -> CommentId {
    CommentId::from_random([byte; 16])
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

fn service(flare_limit: usize) -> Memory {
    let sequence = MemorySequence::new();
    Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::with_sequence(sequence.clone()),
        MemoryCommentStore::with_sequence(sequence).with_flare_limit(flare_limit),
        MemoryCursorStore::new().with_limit(2),
        Creators::Everyone,
    )
}

/// A service in which alice shared flare 1 with bob.
fn with_flare() -> Memory {
    let flares = service(2_000);
    let flare = NewFlare::new(
        flare_id(),
        ShareTitle::try_from("Flare").unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        vec![email("bob@example.com")],
        Vec::new(),
    );
    block_on(flares.share(&alice(), flare, at(1))).unwrap();
    flares
}

fn draft(byte: u8, parent: Option<u8>) -> CommentDraft {
    CommentDraft::new(
        comment_id(byte),
        flare_id(),
        parent.map(comment_id),
        CommentBody::try_from(format!("words {byte}").as_str()).unwrap(),
        CommentKey::try_from(format!("key-{byte}").as_str()).unwrap(),
    )
}

fn post(flares: &Memory, who: &Principal, byte: u8, parent: Option<u8>) {
    block_on(flares.post_comment(who, draft(byte, parent), at(10 + u64::from(byte)))).unwrap();
}

#[test]
fn a_reader_comments_and_anyone_who_can_open_the_flare_reads_the_thread() {
    let flares = with_flare();
    post(&flares, &bob(), 1, None);
    post(&flares, &alice(), 2, Some(1));
    post(&flares, &alice(), 3, None);
    let threads = block_on(flares.comments(&alice(), &flare_id())).unwrap();
    let shape: Vec<(CommentId, Vec<CommentId>)> = threads
        .iter()
        .map(|thread| {
            (
                thread.comment.id.clone(),
                thread
                    .replies
                    .iter()
                    .map(|reply| reply.id.clone())
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            (comment_id(1), vec![comment_id(2)]),
            (comment_id(3), vec![])
        ]
    );
    assert_eq!(threads[0].comment.author.as_str(), "bob@example.com");
    let seen_by_bob = block_on(flares.comments(&bob(), &flare_id())).unwrap();
    assert_eq!(seen_by_bob, threads);
}

#[test]
fn a_person_who_cannot_open_the_flare_gets_not_found_for_reading_and_writing() {
    let flares = with_flare();
    post(&flares, &bob(), 1, None);
    let read = block_on(flares.comments(&outsider(), &flare_id()));
    let write = block_on(flares.post_comment(&outsider(), draft(2, None), at(20)));
    let missing = block_on(flares.comments(&alice(), &ShareId::from_random([9; 16])));
    assert_eq!(read.unwrap_err(), FlareError::NotFound);
    assert_eq!(write.unwrap_err(), FlareError::NotFound);
    assert_eq!(missing.unwrap_err(), FlareError::NotFound);
    let edit = block_on(flares.edit_comment(
        &outsider(),
        &comment_id(1),
        CommentBody::try_from("mine now").unwrap(),
        at(21),
    ));
    assert_eq!(edit.unwrap_err(), FlareError::CommentNotFound);
}

#[test]
fn a_reply_to_a_comment_is_allowed_and_a_reply_to_a_reply_or_a_foreign_comment_is_not() {
    let flares = with_flare();
    post(&flares, &bob(), 1, None);
    post(&flares, &alice(), 2, Some(1));
    let deeper = block_on(flares.post_comment(&bob(), draft(3, Some(2)), at(30)));
    assert_eq!(deeper.unwrap_err(), FlareError::ReplyToReply);
    let unknown = block_on(flares.post_comment(&bob(), draft(4, Some(77)), at(31)));
    assert_eq!(unknown.unwrap_err(), FlareError::CommentNotFound);
    let threads = block_on(flares.comments(&alice(), &flare_id())).unwrap();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].replies.len(), 1);
}

#[test]
fn sending_one_key_twice_answers_the_first_comment() {
    let flares = with_flare();
    let first = block_on(flares.post_comment(&bob(), draft(1, None), at(10))).unwrap();
    let again = CommentDraft::new(
        comment_id(2),
        flare_id(),
        None,
        CommentBody::try_from("other words").unwrap(),
        CommentKey::try_from("key-1").unwrap(),
    );
    let second = block_on(flares.post_comment(&bob(), again, at(11))).unwrap();
    assert!(first.created);
    assert!(!second.created);
    assert_eq!(second.comment, first.comment);
    assert_eq!(
        block_on(flares.comments(&bob(), &flare_id()))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn only_the_author_edits_or_deletes_and_an_owner_is_refused_too() {
    let flares = with_flare();
    post(&flares, &bob(), 1, None);
    let body = CommentBody::try_from("edited").unwrap();
    let by_owner = block_on(flares.edit_comment(&alice(), &comment_id(1), body.clone(), at(20)));
    assert_eq!(by_owner.unwrap_err(), FlareError::NotCommentAuthor);
    let delete = block_on(flares.delete_comment(&alice(), &comment_id(1), at(21)));
    assert_eq!(delete.unwrap_err(), FlareError::NotCommentAuthor);
    let edited = block_on(flares.edit_comment(&bob(), &comment_id(1), body, at(22))).unwrap();
    assert_eq!(
        edited.body.as_ref().map(CommentBody::as_str),
        Some("edited")
    );
    assert_eq!(edited.edited, Some(at(22)));
}

#[test]
fn a_deleted_comment_loses_its_text_and_keeps_its_replies() {
    let flares = with_flare();
    post(&flares, &bob(), 1, None);
    post(&flares, &alice(), 2, Some(1));
    let deleted = block_on(flares.delete_comment(&bob(), &comment_id(1), at(30))).unwrap();
    assert!(deleted.is_deleted());
    let threads = block_on(flares.comments(&alice(), &flare_id())).unwrap();
    assert!(threads[0].comment.is_deleted());
    assert_eq!(threads[0].replies.len(), 1);
    let edit = block_on(flares.edit_comment(
        &bob(),
        &comment_id(1),
        CommentBody::try_from("back").unwrap(),
        at(31),
    ));
    assert_eq!(edit.unwrap_err(), FlareError::CommentNotFound);
}

#[test]
fn an_author_whose_access_ended_can_no_longer_edit() {
    let flares = with_flare();
    post(&flares, &bob(), 1, None);
    block_on(
        flares
            .shares()
            .revoke(&alice(), &flare_id(), &email("bob@example.com")),
    )
    .unwrap();
    let edit = block_on(flares.edit_comment(
        &bob(),
        &comment_id(1),
        CommentBody::try_from("still here").unwrap(),
        at(40),
    ));
    assert_eq!(edit.unwrap_err(), FlareError::CommentNotFound);
}

#[test]
fn a_flare_at_its_comment_limit_refuses_the_next() {
    let flares = service(2);
    let flare = NewFlare::new(
        flare_id(),
        ShareTitle::try_from("Flare").unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        Vec::new(),
        Vec::new(),
    );
    block_on(flares.share(&alice(), flare, at(1))).unwrap();
    post(&flares, &alice(), 1, None);
    post(&flares, &alice(), 2, Some(1));
    let over = block_on(flares.post_comment(&alice(), draft(3, None), at(30)));
    assert_eq!(over.unwrap_err(), FlareError::TooManyComments);
}

#[test]
fn activity_orders_comment_events_with_response_links_by_position() {
    let flares = with_flare();
    post(&flares, &bob(), 1, None);
    let response = NewFlare::new(
        ShareId::from_random([2; 16]),
        ShareTitle::try_from("Response").unwrap(),
        ShareContent::Document(DocumentText::try_from("# Reply\n").unwrap()),
        vec![email("alice@example.com")],
        vec![flare_id()],
    );
    block_on(flares.share(&bob(), response, at(20))).unwrap();
    post(&flares, &alice(), 2, Some(1));
    block_on(flares.edit_comment(
        &bob(),
        &comment_id(1),
        CommentBody::try_from("edited").unwrap(),
        at(40),
    ))
    .unwrap();
    block_on(flares.delete_comment(&alice(), &comment_id(2), at(41))).unwrap();
    let page = block_on(flares.activity(&alice(), &[flare_id()], Position::new(0), 50)).unwrap();
    let kinds: Vec<String> = page
        .items
        .iter()
        .map(|item| match item {
            ActivityItem::Response(_) => "response".to_owned(),
            ActivityItem::Comment(item) => {
                format!("{:?}{}", item.action, item.parent.is_some())
            }
            _ => "other".to_owned(),
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            "Createdfalse",
            "response",
            "Createdtrue",
            "Editedfalse",
            "Deletedtrue"
        ]
    );
    let positions: Vec<Position> = page.items.iter().map(ActivityItem::position).collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    let ActivityItem::Comment(edited) = &page.items[3] else {
        panic!("not a comment")
    };
    assert_eq!(edited.action, CommentAction::Edited);
    assert_eq!(
        edited.body.as_ref().map(CommentBody::as_str),
        Some("edited")
    );
    let ActivityItem::Comment(deleted) = &page.items[4] else {
        panic!("not a comment")
    };
    assert!(deleted.body.is_none());
    // A cursor at the response skips what came before it.
    let since = block_on(flares.activity(&alice(), &[flare_id()], positions[1], 50)).unwrap();
    assert_eq!(since.items.len(), 3);
}

#[test]
fn activity_pages_through_comment_events_and_says_more_when_cut() {
    let flares = with_flare();
    for byte in 1..=5 {
        post(&flares, &bob(), byte, None);
    }
    let mut after = Position::new(0);
    let mut seen = Vec::new();
    loop {
        let page = block_on(flares.activity(&alice(), &[flare_id()], after, 2)).unwrap();
        seen.extend(page.items.iter().map(ActivityItem::position));
        after = page.last;
        if !page.more {
            break;
        }
    }
    assert_eq!(seen.len(), 5);
}

#[test]
fn a_cursor_is_read_advanced_and_never_moves_back() {
    let flares = with_flare();
    let name = CursorName::try_from("inbox").unwrap();
    let missing = block_on(flares.cursor(&alice(), &name));
    assert_eq!(missing.unwrap_err(), FlareError::CursorNotFound);
    let held = block_on(flares.advance_cursor(&alice(), &name, Position::new(9))).unwrap();
    assert_eq!(held, Position::new(9));
    let lower = block_on(flares.advance_cursor(&alice(), &name, Position::new(3))).unwrap();
    assert_eq!(lower, Position::new(9));
    assert_eq!(
        block_on(flares.cursor(&alice(), &name)).unwrap(),
        Position::new(9)
    );
    assert_eq!(
        block_on(flares.cursor(&bob(), &name)).unwrap_err(),
        FlareError::CursorNotFound
    );
    for other in ["a", "b"] {
        let name = CursorName::try_from(other).unwrap();
        let _ = block_on(flares.advance_cursor(&alice(), &name, Position::new(1)));
    }
    let third = CursorName::try_from("c").unwrap();
    assert_eq!(
        block_on(flares.advance_cursor(&alice(), &third, Position::new(1))).unwrap_err(),
        FlareError::TooManyCursors
    );
}

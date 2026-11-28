//! Comments on highlighted text through the `Flares` service over the
//! in-memory stores: an anchor is stored with its comment and read back
//! with it, a delete erases it, and each refusal has a twin that the
//! service admits. Every refusal is checked before any write.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemorySequence,
    MemoryShareStore,
};
use flare_core::{
    AllAdmitted, Anchor, CommentBody, CommentDraft, CommentId, CommentKey, DocumentText, FileId,
    FlareError, Flares, HtmlFile, NewFlare, Person, Principal, PrincipalId, Quote, QuoteContext,
    ShareContent, ShareId, ShareTitle, ShareVersion, Timestamp,
};

type Memory = Flares<
    MemoryShareStore,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
>;

fn email(text: &str) -> Person {
    Person::try_from(text).unwrap()
}

fn alice() -> Principal {
    Principal::with_person(
        PrincipalId::try_from("alice@example.com").unwrap(),
        email("alice@example.com"),
        None,
    )
}

fn markdown() -> ShareId {
    ShareId::from_random([1; 16])
}

fn uploaded() -> ShareId {
    ShareId::from_random([2; 16])
}

fn comment_id(byte: u8) -> CommentId {
    CommentId::from_random([byte; 16])
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

/// A service in which alice shared one Markdown flare and one uploaded
/// HTML flare.
fn with_flares() -> Memory {
    let sequence = MemorySequence::new();
    let flares = Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::with_sequence(sequence.clone()),
        MemoryCommentStore::with_sequence(sequence),
        MemoryCursorStore::new(),
        AllAdmitted,
    );
    let title = || ShareTitle::try_from("Sailings").unwrap();
    let document = ShareContent::Document(DocumentText::try_from("# Sailings\n").unwrap());
    let html = ShareContent::Html(HtmlFile::new(FileId::from_random([4; 16]), 5).unwrap());
    for (id, content) in [(markdown(), document), (uploaded(), html)] {
        let flare = NewFlare::new(id, title(), content, Vec::new(), Vec::new());
        block_on(flares.share(&alice(), flare, at(1))).unwrap();
    }
    flares
}

fn anchor(version: u32) -> Anchor {
    Anchor::new(
        Quote::try_from("the sailing is held").unwrap(),
        QuoteContext::try_from("above the limit, ").unwrap(),
        QuoteContext::try_from(" and passengers").unwrap(),
        ShareVersion::try_from(version).unwrap(),
    )
}

fn draft(byte: u8, flare: ShareId, parent: Option<u8>) -> CommentDraft {
    CommentDraft::new(
        comment_id(byte),
        flare,
        parent.map(comment_id),
        CommentBody::try_from(format!("words {byte}").as_str()).unwrap(),
        CommentKey::try_from(format!("key-{byte}").as_str()).unwrap(),
    )
}

fn post(flares: &Memory, draft: CommentDraft) -> Result<flare_core::Posted, FlareError> {
    block_on(flares.post_comment(&alice(), draft, at(10)))
}

fn stored(flares: &Memory, flare: &ShareId) -> usize {
    block_on(flares.comments(&alice(), flare))
        .unwrap()
        .iter()
        .map(|thread| 1 + thread.replies.len())
        .sum()
}

#[test]
fn an_anchored_comment_reads_back_with_its_quote_context_and_version() {
    let flares = with_flares();
    let posted = post(
        &flares,
        draft(1, markdown(), None).with_anchor(Some(anchor(1))),
    )
    .unwrap();
    assert_eq!(posted.comment.anchor, Some(anchor(1)));
    let threads = block_on(flares.comments(&alice(), &markdown())).unwrap();
    assert_eq!(threads[0].comment.anchor, Some(anchor(1)));
}

#[test]
fn an_anchor_on_a_reply_is_refused_and_writes_nothing_and_the_same_reply_without_one_is_admitted() {
    let flares = with_flares();
    post(&flares, draft(1, markdown(), None)).unwrap();
    let refused = post(
        &flares,
        draft(2, markdown(), Some(1)).with_anchor(Some(anchor(1))),
    );
    assert_eq!(refused.unwrap_err(), FlareError::AnchorOnReply);
    assert_eq!(stored(&flares, &markdown()), 1);
    let admitted = post(&flares, draft(2, markdown(), Some(1))).unwrap();
    assert_eq!(admitted.comment.anchor, None);
}

#[test]
fn an_anchor_on_an_uploaded_html_flare_is_refused_and_the_same_comment_without_one_is_admitted() {
    let flares = with_flares();
    let refused = post(
        &flares,
        draft(1, uploaded(), None).with_anchor(Some(anchor(1))),
    );
    assert_eq!(refused.unwrap_err(), FlareError::AnchorKind);
    assert_eq!(stored(&flares, &uploaded()), 0);
    post(&flares, draft(1, uploaded(), None)).unwrap();
}

#[test]
fn an_anchor_on_a_version_the_flare_does_not_have_is_refused_and_one_on_its_version_is_admitted() {
    let flares = with_flares();
    let refused = post(
        &flares,
        draft(1, markdown(), None).with_anchor(Some(anchor(2))),
    );
    assert_eq!(refused.unwrap_err(), FlareError::AnchorInvalid);
    assert_eq!(stored(&flares, &markdown()), 0);
    post(
        &flares,
        draft(1, markdown(), None).with_anchor(Some(anchor(1))),
    )
    .unwrap();
}

#[test]
fn a_delete_erases_the_anchor_with_the_text_and_keeps_the_replies() {
    let flares = with_flares();
    post(
        &flares,
        draft(1, markdown(), None).with_anchor(Some(anchor(1))),
    )
    .unwrap();
    post(&flares, draft(2, markdown(), Some(1))).unwrap();
    let deleted = block_on(flares.delete_comment(&alice(), &comment_id(1), at(20))).unwrap();
    assert_eq!(deleted.anchor, None);
    assert_eq!(deleted.body, None);
    let threads = block_on(flares.comments(&alice(), &markdown())).unwrap();
    assert_eq!(threads[0].comment.anchor, None);
    assert_eq!(threads[0].replies.len(), 1);
}

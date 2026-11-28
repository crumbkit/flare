//! Runs the comment and cursor suites against `DynamoCommentStore` and
//! `DynamoCursorStore` on the DynamoDB stand-in, and checks what only a
//! durable table shows: comments and responses share one position counter,
//! a second store on the same table sees every write, and a lost answer or
//! a dropped request behaves as the stores promise.
#![cfg(test)]

#[path = "support/stand_in.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the stand-in support is test support: it panics on a broken fixture"
)]
pub mod stand_in;

use flare_aws::{Fault, Stores};
use flare_core::contract::comment_store::{self, CommentFixture};
use flare_core::contract::cursor_store::{self, CursorFixture};
use flare_core::{
    Added, CommentBody, CommentId, CommentKey, CommentStore, CommentStoreError, CursorName,
    CursorStore, NewComment, NewLink, Position, PrincipalId, ResponseStore, ShareId, ShareVersion,
    Timestamp,
};
use stand_in::{Joined, endpoints, fresh, fresh_config};

macro_rules! suite {
    ($module:ident, $field:ident, $fixture:ident; $($name:ident),* $(,)?) => {$(
        #[tokio::test(flavor = "multi_thread")]
        async fn $name() {
            let fixture = $fixture::standard().unwrap();
            let stores = fresh().await;
            $module::$name(&stores.$field, &fixture).await.unwrap();
        }
    )*};
}

macro_rules! concurrent_suite {
    ($module:ident, $field:ident, $fixture:ident; $($name:ident),* $(,)?) => {$(
        #[tokio::test(flavor = "multi_thread")]
        async fn $name() {
            let fixture = $fixture::standard().unwrap();
            let stores = fresh().await;
            $module::$name(&stores.$field, &fixture, &Joined).await.unwrap();
        }
    )*};
}

mod comments {
    use super::*;

    suite!(comment_store, comments, CommentFixture;
        a_comment_reads_back,
        a_flares_list_holds_only_its_comments,
        every_change_is_an_event_at_a_larger_position,
        events_are_bounded_by_position_and_limit,
        a_repeated_key_answers_the_first_comment,
        an_edit_replaces_the_text_and_keeps_the_rest,
        a_delete_erases_the_text_and_keeps_the_rest,
        an_anchor_reads_back_and_a_delete_erases_it,
        an_unknown_comment_is_not_found,
    );

    concurrent_suite!(comment_store, comments, CommentFixture;
        concurrent_comments_get_distinct_positions,
        concurrent_edit_and_delete_leave_one_consistent_state,
    );

    #[tokio::test(flavor = "multi_thread")]
    async fn the_flare_limit_holds() {
        let fixture = CommentFixture::standard().unwrap();
        let stores = fresh().await;
        let store = stores.comments.clone().with_flare_limit(3);
        comment_store::the_flare_limit_holds(&store, &fixture, 3)
            .await
            .unwrap();
    }
}

mod cursors {
    use super::*;

    suite!(cursor_store, cursors, CursorFixture;
        an_advance_makes_the_cursor,
        a_cursor_never_moves_back,
        cursors_are_kept_apart,
    );

    #[tokio::test(flavor = "multi_thread")]
    async fn the_cursor_limit_holds() {
        let fixture = CursorFixture::standard().unwrap();
        let stores = fresh().await;
        let store = stores.cursors.clone().with_limit(3);
        cursor_store::the_cursor_limit_holds(&store, &fixture, 3)
            .await
            .unwrap();
    }
}

fn flare() -> ShareId {
    ShareId::from_random([1; 16])
}

fn new_comment(tag: u8, key: &str) -> NewComment {
    NewComment::new(
        CommentId::from_random([tag; 16]),
        flare(),
        None,
        PrincipalId::try_from("ana@example.org").unwrap(),
        CommentBody::try_from("words").unwrap(),
        CommentKey::try_from(key).unwrap(),
        Timestamp::from_secs(10),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn comments_and_response_links_take_positions_from_one_counter() {
    let stores = fresh().await;
    let link = NewLink::new(
        ShareId::from_random([2; 16]),
        flare(),
        ShareVersion::first(),
        Timestamp::from_secs(5),
    );
    let first = stores.responses.link(link).await.unwrap();
    let Added::Created(comment) = stores.comments.add(new_comment(1, "one")).await.unwrap() else {
        panic!("not created");
    };
    let edited = stores
        .comments
        .edit(
            &comment.id,
            CommentBody::try_from("again").unwrap(),
            Timestamp::from_secs(11),
        )
        .await
        .unwrap();
    assert!(comment.position > first.position);
    assert_eq!(edited.position, comment.position);
    let head = stores.responses.head().await.unwrap();
    assert_eq!(head, Position::new(first.position.get() + 2));
    let events = stores
        .comments
        .events(&flare(), Position::new(0), head, 10)
        .await
        .unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(events.last().map(|event| event.position), Some(head));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_store_on_the_same_table_sees_a_comment_its_edit_and_a_cursor() {
    let config = fresh_config().await;
    let one = Stores::connect_at(&config, &endpoints());
    let two = Stores::connect_at(&config, &endpoints());
    let Added::Created(comment) = one.comments.add(new_comment(1, "one")).await.unwrap() else {
        panic!("not created");
    };
    one.comments
        .edit(
            &comment.id,
            CommentBody::try_from("edited").unwrap(),
            Timestamp::from_secs(11),
        )
        .await
        .unwrap();
    let seen = two.comments.get(&comment.id).await.unwrap();
    assert_eq!(seen.body.as_ref().map(CommentBody::as_str), Some("edited"));
    assert_eq!(two.comments.of_flare(&flare()).await.unwrap().len(), 1);
    let owner = PrincipalId::try_from("ana@example.org").unwrap();
    let name = CursorName::try_from("inbox").unwrap();
    one.cursors
        .advance(&owner, &name, Position::new(4))
        .await
        .unwrap();
    assert_eq!(
        two.cursors.get(&owner, &name).await.unwrap(),
        Some(Position::new(4))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_comment_edit_delete_and_cursor_whose_answers_are_lost_answer_success() {
    let config = fresh_config().await;
    let healthy = Stores::connect_at(&config, &endpoints());
    let faulty = Stores::connect_at_with_fault(&config, &endpoints(), Fault::LoseEveryAnswer);
    let added = faulty.comments.add(new_comment(1, "one")).await.unwrap();
    assert!(matches!(added, Added::Created(_)));
    let id = CommentId::from_random([1; 16]);
    let edited = faulty
        .comments
        .edit(
            &id,
            CommentBody::try_from("edited").unwrap(),
            Timestamp::from_secs(11),
        )
        .await
        .unwrap();
    assert_eq!(
        edited.body.as_ref().map(CommentBody::as_str),
        Some("edited")
    );
    let deleted = faulty
        .comments
        .delete(&id, Timestamp::from_secs(12))
        .await
        .unwrap();
    assert!(deleted.is_deleted());
    // One create, one edit and one delete took three positions, once each.
    assert_eq!(healthy.responses.head().await.unwrap(), Position::new(3));
    let owner = PrincipalId::try_from("ana@example.org").unwrap();
    let name = CursorName::try_from("inbox").unwrap();
    let held = faulty
        .cursors
        .advance(&owner, &name, Position::new(7))
        .await
        .unwrap();
    assert_eq!(held, Position::new(7));
    assert_eq!(
        healthy.cursors.get(&owner, &name).await.unwrap(),
        Some(Position::new(7))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_comment_whose_request_never_arrived_answers_unavailable_and_writes_nothing() {
    let config = fresh_config().await;
    let healthy = Stores::connect_at(&config, &endpoints());
    let faulty = Stores::connect_at_with_fault(&config, &endpoints(), Fault::DropEveryRequest);
    let added = faulty.comments.add(new_comment(1, "one")).await;
    assert_eq!(added.unwrap_err(), CommentStoreError::Unavailable);
    assert!(
        healthy
            .comments
            .of_flare(&flare())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(healthy.responses.head().await.unwrap(), Position::new(0));
}

//! The comment store clauses about edits and deletes.

use super::{ALL, CommentFixture, body, comment_id, flare_id};
use crate::contract::{Clause, RunTogether, Violation, ensure, unexpected};
use crate::{
    Added, CommentAction, CommentBody, CommentStore, CommentStoreError, Position, Timestamp,
};

/// An edit replaces the text and sets the edit time, and keeps the
/// author, the parent, the creation time and the position.
///
/// # Errors
///
/// Returns [`Clause::AnEditReplacesTheTextAndKeepsTheRest`] otherwise.
pub async fn an_edit_replaces_the_text_and_keeps_the_rest<C: CommentStore>(
    store: &C,
    contract_fixture: &CommentFixture,
) -> Result<(), Violation> {
    unexpected(store.add(contract_fixture.comment(1, 1, None)?).await)?;
    let Added::Created(before) =
        unexpected(store.add(contract_fixture.comment(2, 1, Some(1))?).await)?
    else {
        return Err(Violation::new(
            Clause::AnEditReplacesTheTextAndKeepsTheRest,
            "not created",
        ));
    };
    let edited = unexpected(
        store
            .edit(
                &comment_id(2),
                body("new words")?,
                Timestamp::from_secs(300),
            )
            .await,
    )?;
    let read = unexpected(store.get(&comment_id(2)).await)?;
    ensure(
        read == edited
            && edited.body.as_ref().map(CommentBody::as_str) == Some("new words")
            && edited.edited == Some(Timestamp::from_secs(300))
            && edited.author == before.author
            && edited.parent == before.parent
            && edited.created == before.created
            && edited.position == before.position,
        Clause::AnEditReplacesTheTextAndKeepsTheRest,
        || format!("before {before:?}, after {edited:?}"),
    )
}

/// A delete erases the text and keeps the rest, a second delete changes
/// nothing and writes no event, and an edit of a deleted comment is
/// refused.
///
/// # Errors
///
/// Returns [`Clause::ADeleteErasesTheTextAndKeepsTheRest`] otherwise.
pub async fn a_delete_erases_the_text_and_keeps_the_rest<C: CommentStore>(
    store: &C,
    contract_fixture: &CommentFixture,
) -> Result<(), Violation> {
    let Added::Created(before) =
        unexpected(store.add(contract_fixture.comment(1, 1, None)?).await)?
    else {
        return Err(Violation::new(
            Clause::ADeleteErasesTheTextAndKeepsTheRest,
            "not created",
        ));
    };
    let deleted = unexpected(
        store
            .delete(&comment_id(1), Timestamp::from_secs(300))
            .await,
    )?;
    let again = unexpected(
        store
            .delete(&comment_id(1), Timestamp::from_secs(400))
            .await,
    )?;
    let refused = store
        .edit(&comment_id(1), body("back")?, Timestamp::from_secs(500))
        .await;
    let events = unexpected(store.events(&flare_id(1), Position::new(0), ALL, 10).await)?;
    ensure(
        deleted.body.is_none()
            && deleted.is_deleted()
            && deleted.author == before.author
            && deleted.created == before.created
            && deleted.position == before.position
            && again == deleted
            && refused == Err(CommentStoreError::Deleted)
            && events.len() == 2
            && unexpected(store.get(&comment_id(1)).await)?.is_deleted(),
        Clause::ADeleteErasesTheTextAndKeepsTheRest,
        || format!("deleted {deleted:?}, again {again:?}, refused {refused:?}, events {events:?}"),
    )
}

/// An unknown comment is not found by a read, an edit or a delete, and
/// nothing is written.
///
/// # Errors
///
/// Returns [`Clause::AnUnknownCommentIsNotFound`] otherwise.
pub async fn an_unknown_comment_is_not_found<C: CommentStore>(
    store: &C,
    _contract_fixture: &CommentFixture,
) -> Result<(), Violation> {
    let read = store.get(&comment_id(9)).await;
    let edit = store
        .edit(&comment_id(9), body("x")?, Timestamp::from_secs(1))
        .await;
    let delete = store.delete(&comment_id(9), Timestamp::from_secs(1)).await;
    let events = unexpected(store.events(&flare_id(1), Position::new(0), ALL, 10).await)?;
    ensure(
        read == Err(CommentStoreError::NotFound)
            && edit == Err(CommentStoreError::NotFound)
            && delete == Err(CommentStoreError::NotFound)
            && events.is_empty(),
        Clause::AnUnknownCommentIsNotFound,
        || format!("{read:?}, {edit:?}, {delete:?}, {events:?}"),
    )
}

/// An edit and a delete that run together leave the comment deleted, and
/// an edit that succeeded left its event.
///
/// # Errors
///
/// Returns [`Clause::ConcurrentEditAndDeleteLeaveOneConsistentState`] otherwise.
pub async fn concurrent_edit_and_delete_leave_one_consistent_state<C: CommentStore>(
    store: &C,
    contract_fixture: &CommentFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    for round in 1..=6_u8 {
        unexpected(
            store
                .add(contract_fixture.comment(round, round, None)?)
                .await,
        )?;
        let id = comment_id(round);
        let new = body("raced")?;
        let answers = runner
            .run_together(
                || store.edit(&id, new.clone(), Timestamp::from_secs(300)),
                || store.delete(&id, Timestamp::from_secs(301)),
            )
            .await;
        let (edit, delete) = unexpected(answers.ok_or("a concurrent call panicked"))?;
        let held = unexpected(store.get(&id).await)?;
        let events = unexpected(
            store
                .events(&flare_id(round), Position::new(0), ALL, 10)
                .await,
        )?;
        let edit_events = events
            .iter()
            .filter(|event| event.action == CommentAction::Edited)
            .count();
        let allowed = matches!(edit, Ok(_) | Err(CommentStoreError::Deleted));
        ensure(
            held.is_deleted()
                && delete.is_ok()
                && allowed
                && edit_events == usize::from(edit.is_ok())
                && events.len() == 2 + edit_events,
            Clause::ConcurrentEditAndDeleteLeaveOneConsistentState,
            || format!("round {round}: edit {edit:?}, delete {delete:?}, events {events:?}"),
        )?;
    }
    Ok(())
}

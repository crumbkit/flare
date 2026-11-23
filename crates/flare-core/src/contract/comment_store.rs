//! The [`CommentStore`] contract suite. Every function writes the comments
//! it needs, so the caller passes a new, empty store.

use super::{Clause, RunTogether, Violation, ensure, fixture, unexpected};
use crate::{
    Added, Comment, CommentAction, CommentBody, CommentId, CommentKey, CommentStore,
    CommentStoreError, NewComment, Position, PrincipalId, ShareId, Timestamp,
};

/// The people, flares and texts that the suite uses.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CommentFixture {
    ana: PrincipalId,
    jo: PrincipalId,
}

impl CommentFixture {
    /// The standard fixture.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by
    /// its type, which means this crate is broken.
    pub fn standard() -> Result<Self, Violation> {
        Ok(Self {
            ana: fixture(PrincipalId::try_from("ana@example.org"))?,
            jo: fixture(PrincipalId::try_from("jo@example.org"))?,
        })
    }

    /// A comment `tag` by Ana on flare `flare`, with the key `key-<tag>`.
    fn comment(&self, tag: u8, flare: u8, parent: Option<u8>) -> Result<NewComment, Violation> {
        self.by(&self.ana, tag, flare, parent, &format!("key-{tag}"))
    }

    fn by(
        &self,
        author: &PrincipalId,
        tag: u8,
        flare: u8,
        parent: Option<u8>,
        key: &str,
    ) -> Result<NewComment, Violation> {
        Ok(NewComment::new(
            comment_id(tag),
            flare_id(flare),
            parent.map(comment_id),
            author.clone(),
            body(&format!("text {tag}"))?,
            fixture(CommentKey::try_from(key))?,
            Timestamp::from_secs(100 + u64::from(tag)),
        ))
    }
}

fn comment_id(tag: u8) -> CommentId {
    CommentId::from_random([tag; 16])
}

fn flare_id(tag: u8) -> ShareId {
    ShareId::from_random([tag; 16])
}

fn body(text: &str) -> Result<CommentBody, Violation> {
    fixture(CommentBody::try_from(text))
}

/// Every position: no upper bound on a read.
const ALL: Position = Position::new(u64::MAX);

fn created(added: Added) -> Option<Comment> {
    match added {
        Added::Created(comment) => Some(comment),
        _ => None,
    }
}

/// A comment reads back by id and in its flare's list with its author,
/// parent, text, time and position, and no edit time.
///
/// # Errors
///
/// Returns [`Clause::ACommentReadsBack`] otherwise.
pub async fn a_comment_reads_back<C: CommentStore>(
    store: &C,
    contract_fixture: &CommentFixture,
) -> Result<(), Violation> {
    let first = unexpected(store.add(contract_fixture.comment(1, 1, None)?).await)?;
    let Some(first) = created(first) else {
        return Err(Violation::new(Clause::ACommentReadsBack, "not created"));
    };
    let reply = unexpected(store.add(contract_fixture.comment(2, 1, Some(1))?).await)?;
    let Some(reply) = created(reply) else {
        return Err(Violation::new(
            Clause::ACommentReadsBack,
            "reply not created",
        ));
    };
    let got = unexpected(store.get(&comment_id(1)).await)?;
    let listed = unexpected(store.of_flare(&flare_id(1)).await)?;
    ensure(
        got == first
            && listed == vec![first.clone(), reply.clone()]
            && first.parent.is_none()
            && reply.parent == Some(comment_id(1))
            && first.author == contract_fixture.ana
            && first.body.as_ref().map(CommentBody::as_str) == Some("text 1")
            && first.edited.is_none()
            && first.created == Timestamp::from_secs(101)
            && reply.position > first.position,
        Clause::ACommentReadsBack,
        || format!("added {first:?} and {reply:?}, got {got:?}, listed {listed:?}"),
    )
}

/// A flare's list holds only that flare's comments, oldest first.
///
/// # Errors
///
/// Returns [`Clause::AFlaresListHoldsOnlyItsComments`] otherwise.
pub async fn a_flares_list_holds_only_its_comments<C: CommentStore>(
    store: &C,
    contract_fixture: &CommentFixture,
) -> Result<(), Violation> {
    for (tag, flare) in [(1, 1), (2, 2), (3, 1), (4, 1)] {
        unexpected(store.add(contract_fixture.comment(tag, flare, None)?).await)?;
    }
    let listed = unexpected(store.of_flare(&flare_id(1)).await)?;
    let ids: Vec<CommentId> = listed.iter().map(|c| c.id.clone()).collect();
    ensure(
        ids == vec![comment_id(1), comment_id(3), comment_id(4)],
        Clause::AFlaresListHoldsOnlyItsComments,
        || format!("{listed:?}"),
    )
}

/// Each create, edit and delete takes a larger position, and its event
/// names the comment, its author and what happened.
///
/// # Errors
///
/// Returns [`Clause::EveryChangeIsAnEventAtALargerPosition`] otherwise.
pub async fn every_change_is_an_event_at_a_larger_position<C: CommentStore>(
    store: &C,
    contract_fixture: &CommentFixture,
) -> Result<(), Violation> {
    unexpected(store.add(contract_fixture.comment(1, 1, None)?).await)?;
    unexpected(store.add(contract_fixture.comment(2, 1, Some(1))?).await)?;
    unexpected(
        store
            .edit(&comment_id(1), body("changed")?, Timestamp::from_secs(200))
            .await,
    )?;
    unexpected(
        store
            .delete(&comment_id(2), Timestamp::from_secs(201))
            .await,
    )?;
    let events = unexpected(store.events(&flare_id(1), Position::new(0), ALL, 10).await)?;
    let shape: Vec<(CommentAction, CommentId)> = events
        .iter()
        .map(|event| (event.action, event.comment.clone()))
        .collect();
    let rising = events.windows(2).all(|pair| {
        pair.first()
            .zip(pair.get(1))
            .is_some_and(|(one, two)| one.position < two.position)
    });
    ensure(
        shape
            == vec![
                (CommentAction::Created, comment_id(1)),
                (CommentAction::Created, comment_id(2)),
                (CommentAction::Edited, comment_id(1)),
                (CommentAction::Deleted, comment_id(2)),
            ]
            && rising
            && events
                .iter()
                .all(|event| event.author == contract_fixture.ana)
            && events.get(1).and_then(|event| event.parent.clone()) == Some(comment_id(1))
            && events.get(2).map(|event| event.at) == Some(Timestamp::from_secs(200)),
        Clause::EveryChangeIsAnEventAtALargerPosition,
        || format!("{events:?}"),
    )
}

/// The events of a flare above a position and up to a bound, at most the
/// limit, oldest first, and none of another flare.
///
/// # Errors
///
/// Returns [`Clause::EventsAreBoundedByPositionAndLimit`] otherwise.
pub async fn events_are_bounded_by_position_and_limit<C: CommentStore>(
    store: &C,
    contract_fixture: &CommentFixture,
) -> Result<(), Violation> {
    let mut positions = Vec::new();
    for tag in 1..=5 {
        let flare = if tag == 3 { 2 } else { 1 };
        let added = unexpected(store.add(contract_fixture.comment(tag, flare, None)?).await)?;
        positions.push(created(added).map(|c| c.position));
    }
    let [Some(first), _, _, Some(fourth), Some(fifth)] = positions[..] else {
        return Err(Violation::new(
            Clause::EventsAreBoundedByPositionAndLimit,
            "no positions",
        ));
    };
    let mid = unexpected(store.events(&flare_id(1), first, fourth, 10).await)?;
    let limited = unexpected(store.events(&flare_id(1), Position::new(0), ALL, 2).await)?;
    let none = unexpected(store.events(&flare_id(1), fifth, ALL, 10).await)?;
    ensure(
        mid.len() == 2
            && mid
                .iter()
                .all(|event| event.position > first && event.position <= fourth)
            && limited.len() == 2
            && limited.iter().all(|event| event.flare == flare_id(1))
            && none.is_empty(),
        Clause::EventsAreBoundedByPositionAndLimit,
        || format!("mid {mid:?}, limited {limited:?}, none {none:?}"),
    )
}

/// A repeat of one author's key on one flare answers the first comment and
/// writes nothing, and another key, author or flare writes a new comment.
///
/// # Errors
///
/// Returns [`Clause::ARepeatedKeyAnswersTheFirstComment`] otherwise.
pub async fn a_repeated_key_answers_the_first_comment<C: CommentStore>(
    store: &C,
    contract_fixture: &CommentFixture,
) -> Result<(), Violation> {
    let first = unexpected(store.add(contract_fixture.comment(1, 1, None)?).await)?;
    let again = unexpected(
        store
            .add(contract_fixture.by(&contract_fixture.ana, 2, 1, None, "key-1")?)
            .await,
    )?;
    let other_person = unexpected(
        store
            .add(contract_fixture.by(&contract_fixture.jo, 3, 1, None, "key-1")?)
            .await,
    )?;
    let other_flare = unexpected(
        store
            .add(contract_fixture.by(&contract_fixture.ana, 4, 2, None, "key-1")?)
            .await,
    )?;
    let Added::Created(first) = first else {
        return Err(Violation::new(
            Clause::ARepeatedKeyAnswersTheFirstComment,
            "not created",
        ));
    };
    let listed = unexpected(store.of_flare(&flare_id(1)).await)?;
    let events = unexpected(store.events(&flare_id(1), Position::new(0), ALL, 10).await)?;
    ensure(
        again == Added::Repeated(first)
            && created(other_person).is_some()
            && created(other_flare).is_some()
            && listed.len() == 2
            && events.len() == 2,
        Clause::ARepeatedKeyAnswersTheFirstComment,
        || format!("again {again:?}, listed {listed:?}, events {events:?}"),
    )
}

/// Two comments that run together get two different positions, and two
/// with one key give one created and one repeated.
///
/// # Errors
///
/// Returns [`Clause::ConcurrentCommentsGetDistinctPositions`] otherwise.
pub async fn concurrent_comments_get_distinct_positions<C: CommentStore>(
    store: &C,
    contract_fixture: &CommentFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    for round in 0..6_u8 {
        let one = contract_fixture.comment(round.saturating_mul(4).saturating_add(1), 1, None)?;
        let two = contract_fixture.comment(round.saturating_mul(4).saturating_add(2), 1, None)?;
        let answers = runner
            .run_together(|| store.add(one.clone()), || store.add(two.clone()))
            .await;
        let (first, second) = unexpected(answers.ok_or("a concurrent add panicked"))?;
        let (first, second) = (created(unexpected(first)?), created(unexpected(second)?));
        ensure(
            first
                .as_ref()
                .zip(second.as_ref())
                .is_some_and(|(one, two)| one.position != two.position),
            Clause::ConcurrentCommentsGetDistinctPositions,
            || format!("round {round}: {first:?} and {second:?}"),
        )?;
    }
    let same = contract_fixture.by(&contract_fixture.jo, 200, 3, None, "same")?;
    let twin = contract_fixture.by(&contract_fixture.jo, 201, 3, None, "same")?;
    let answers = runner
        .run_together(|| store.add(same.clone()), || store.add(twin.clone()))
        .await;
    let (first, second) = unexpected(answers.ok_or("a concurrent add panicked"))?;
    let (first, second) = (unexpected(first)?, unexpected(second)?);
    let listed = unexpected(store.of_flare(&flare_id(3)).await)?;
    let one_each = matches!(
        (&first, &second),
        (Added::Created(_), Added::Repeated(_)) | (Added::Repeated(_), Added::Created(_))
    );
    ensure(
        one_each && listed.len() == 1,
        Clause::ConcurrentCommentsGetDistinctPositions,
        || format!("same key: {first:?} and {second:?}, listed {listed:?}"),
    )
}

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

/// A flare that holds `limit` comments refuses the next with too many and
/// writes nothing, a repeat of an earlier key still answers the first
/// comment, and another flare is not affected. The caller builds the store
/// with that limit.
///
/// # Errors
///
/// Returns [`Clause::TheFlareLimitHolds`] otherwise.
pub async fn the_flare_limit_holds<C: CommentStore>(
    store: &C,
    contract_fixture: &CommentFixture,
    limit: u8,
) -> Result<(), Violation> {
    for tag in 1..=limit {
        unexpected(store.add(contract_fixture.comment(tag, 1, None)?).await)?;
    }
    let over = store
        .add(contract_fixture.comment(limit.saturating_add(1), 1, None)?)
        .await;
    let repeat = unexpected(store.add(contract_fixture.comment(2, 1, None)?).await)?;
    let other = unexpected(
        store
            .add(contract_fixture.comment(limit.saturating_add(2), 2, None)?)
            .await,
    )?;
    let listed = unexpected(store.of_flare(&flare_id(1)).await)?;
    ensure(
        over == Err(CommentStoreError::TooMany)
            && matches!(repeat, Added::Repeated(_))
            && created(other).is_some()
            && listed.len() == usize::from(limit),
        Clause::TheFlareLimitHolds,
        || format!("over {over:?}, repeat {repeat:?}, listed {}", listed.len()),
    )
}

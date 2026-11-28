//! The comment store clause about anchors: the quote, context and version
//! a comment on highlighted text holds.

use super::{CommentFixture, body, comment_id, flare_id};
use crate::contract::{Clause, Violation, ensure, fixture, unexpected};
use crate::{Added, Anchor, CommentStore, Quote, QuoteContext, ShareVersion, Timestamp};

fn anchor() -> Result<Anchor, Violation> {
    Ok(Anchor::new(
        fixture(Quote::try_from("the sailing is held"))?,
        fixture(QuoteContext::try_from("above the limit, "))?,
        fixture(QuoteContext::try_from(" and passengers"))?,
        fixture(ShareVersion::try_from(2))?,
    ))
}

/// A comment's anchor reads back by id and in its flare's list, an edit
/// keeps it, and a delete erases it with the text and keeps the reply that
/// hangs from the comment.
///
/// # Errors
///
/// Returns [`Clause::AnAnchorReadsBackAndADeleteErasesIt`] otherwise.
pub async fn an_anchor_reads_back_and_a_delete_erases_it<C: CommentStore>(
    store: &C,
    contract_fixture: &CommentFixture,
) -> Result<(), Violation> {
    let clause = Clause::AnAnchorReadsBackAndADeleteErasesIt;
    let new = contract_fixture
        .comment(1, 1, None)?
        .with_anchor(Some(anchor()?));
    let Added::Created(first) = unexpected(store.add(new).await)? else {
        return Err(Violation::new(clause, "not created"));
    };
    unexpected(store.add(contract_fixture.comment(2, 1, Some(1))?).await)?;
    let got = unexpected(store.get(&comment_id(1)).await)?;
    let listed = unexpected(store.of_flare(&flare_id(1)).await)?;
    let edited = unexpected(
        store
            .edit(
                &comment_id(1),
                body("new words")?,
                Timestamp::from_secs(300),
            )
            .await,
    )?;
    let deleted = unexpected(
        store
            .delete(&comment_id(1), Timestamp::from_secs(400))
            .await,
    )?;
    let after = unexpected(store.get(&comment_id(1)).await)?;
    let replies = unexpected(store.of_flare(&flare_id(1)).await)?
        .into_iter()
        .filter(|comment| comment.parent == Some(comment_id(1)))
        .count();
    let expected = Some(anchor()?);
    ensure(
        first.anchor == expected
            && got.anchor == expected
            && listed.first().map(|comment| &comment.anchor) == Some(&expected)
            && listed.get(1).is_some_and(|reply| reply.anchor.is_none())
            && edited.anchor == expected
            && deleted.anchor.is_none()
            && after.anchor.is_none()
            && replies == 1,
        clause,
        || {
            format!(
                "added {first:?}, got {got:?}, listed {listed:?}, edited {edited:?}, deleted {deleted:?}, after {after:?}, {replies} replies"
            )
        },
    )
}

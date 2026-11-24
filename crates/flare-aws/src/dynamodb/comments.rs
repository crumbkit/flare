//! [`DynamoCommentStore`], flare-core's `CommentStore` on the table.
//!
//! Every create, edit and delete is one transaction that moves the
//! response store's position counter and writes the comment's items, so a
//! comment event is visible exactly when its position is the head, as a
//! response link is.

mod items;

use crate::dynamodb::Table;
use crate::dynamodb::cancel::Classified;
use crate::dynamodb::items::{ItemKind, Rev};
use crate::dynamodb::keys::{COMMENT_PREFIX, Key, comment_partition, event_sort_key};
use crate::dynamodb::responses::read_head;
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::dynamodb::settle::malformed;
use crate::dynamodb::transact::{ItemRole, Transaction};
use flare_core::{
    Added, Comment, CommentAction, CommentBody, CommentEvent, CommentId, CommentStore,
    CommentStoreError, MAX_COMMENTS_PER_FLARE, NewComment, Position, ShareId, Timestamp,
};
use items::{comment_from, event_from, pointer_from, slot_from};

/// Keeps the comments and replies of flares in the table.
///
/// Every create, edit and delete moves the one position counter, so they
/// are written one at a time across the whole table. A write that meets
/// another write's move of the counter reads the counter again and takes
/// the next position.
#[derive(Debug, Clone)]
pub struct DynamoCommentStore {
    table: Table,
    limit: usize,
}

impl DynamoCommentStore {
    pub(crate) fn from_table(table: Table) -> Self {
        Self {
            table,
            limit: MAX_COMMENTS_PER_FLARE,
        }
    }

    /// The same store with `limit` comments per flare, so a test reaches
    /// the limit with a few comments.
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn with_flare_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

type Unavailable = ();

/// Reads the comment `id`, or `None` when no pointer names it.
async fn read_comment(
    table: &Table,
    id: &CommentId,
) -> Result<Option<(Comment, Rev)>, Unavailable> {
    let Some(pointer) = table
        .get(&Key::comment_pointer(id), ItemKind::CommentPointer)
        .await
        .map_err(|_| ())?
    else {
        return Ok(None);
    };
    let (flare, position) = pointer_from(&pointer).map_err(malformed)?;
    let item = table
        .get(&Key::comment(&flare, position), ItemKind::Comment)
        .await
        .map_err(|_| ())?
        .ok_or_else(|| malformed(crate::dynamodb::items::Malformed(ItemKind::Comment)))?;
    comment_from(&item).map(Some).map_err(malformed)
}

/// The comment that the key slot of `new` names, when there is one.
async fn read_slot(table: &Table, new: &NewComment) -> Result<Option<Comment>, Unavailable> {
    let Some(slot) = table
        .get(&items::slot_key(new), ItemKind::CommentKeySlot)
        .await
        .map_err(|_| ())?
    else {
        return Ok(None);
    };
    let id = slot_from(&slot).map_err(malformed)?;
    Ok(read_comment(table, &id).await?.map(|(comment, _)| comment))
}

fn next_position(head: Position) -> Option<Position> {
    head.get().checked_add(1).map(Position::new)
}

/// A new comment.
struct AddOp<'a> {
    table: &'a Table,
    new: &'a NewComment,
    limit: usize,
}

struct BuiltAdd {
    transaction: Transaction,
    comment: Comment,
}

impl Attempt for AddOp<'_> {
    type Built = BuiltAdd;
    type Done = Added;
    type Refused = CommentStoreError;

    async fn build(&self) -> Result<BuiltAdd, Sent<Added, CommentStoreError>> {
        match read_slot(self.table, self.new).await {
            Ok(Some(first)) => return Err(Sent::Done(Added::Repeated(first))),
            Ok(None) => {}
            Err(()) => return Err(Sent::Unavailable),
        }
        let head = read_head(self.table)
            .await
            .map_err(|()| Sent::Unavailable)?;
        let position = next_position(head).ok_or(Sent::Unavailable)?;
        let comment = Comment::from_new(self.new.clone(), position);
        let rev = Rev::fresh().ok_or(Sent::Unavailable)?;
        let transaction = items::add(
            self.table.name(),
            self.new,
            &comment,
            &rev,
            head,
            self.limit,
        )
        .ok_or(Sent::Unavailable)?;
        Ok(BuiltAdd {
            transaction,
            comment,
        })
    }

    async fn send(&self, built: &BuiltAdd, token: &Token) -> Sent<Added, CommentStoreError> {
        match self
            .table
            .transact(&built.transaction, token, "comment")
            .await
        {
            Ok(()) => Sent::Done(Added::Created(built.comment.clone())),
            Err(Classified::Conflict {
                role: ItemRole::CommentCount,
                ..
            }) => Sent::Refused(CommentStoreError::TooMany),
            Err(Classified::Conflict {
                role: ItemRole::CommentPointer,
                ..
            }) => Sent::Unavailable,
            // The counter moved, or the author's key slot was written, since
            // the read; a fresh read takes the next position or finds the slot.
            Err(Classified::Conflict { .. } | Classified::Retry) => Sent::Again,
            Err(Classified::Unknown) => Sent::Unknown,
            Err(Classified::Unavailable(_)) => Sent::Unavailable,
        }
    }

    /// The comment item at the built position holds this comment's id when
    /// the transaction applied.
    async fn settle(&self, built: &BuiltAdd) -> Settled<Added> {
        let key = Key::comment(&built.comment.flare, built.comment.position);
        let Ok(item) = self.table.get(&key, ItemKind::Comment).await else {
            return Settled::Unavailable;
        };
        match item.as_ref().map(comment_from) {
            None => Settled::Differs,
            Some(Ok((held, _))) if held.id == built.comment.id => {
                Settled::Matches(Added::Created(held))
            }
            Some(Ok(_)) => Settled::Differs,
            Some(Err(error)) => {
                malformed(error);
                Settled::Unavailable
            }
        }
    }
}

/// What a change does to a comment's text.
enum Change {
    Edit(CommentBody),
    Delete,
}

/// An edit or a delete.
struct ChangeOp<'a> {
    table: &'a Table,
    id: &'a CommentId,
    change: &'a Change,
    at: Timestamp,
}

struct BuiltChange {
    transaction: Transaction,
    after: Comment,
    next: Rev,
}

impl Attempt for ChangeOp<'_> {
    type Built = BuiltChange;
    type Done = Comment;
    type Refused = CommentStoreError;

    async fn build(&self) -> Result<BuiltChange, Sent<Comment, CommentStoreError>> {
        let (held, rev) = match read_comment(self.table, self.id).await {
            Ok(Some(found)) => found,
            Ok(None) => return Err(Sent::Refused(CommentStoreError::NotFound)),
            Err(()) => return Err(Sent::Unavailable),
        };
        let mut after = held.clone();
        let action = match self.change {
            Change::Edit(body) => {
                if held.is_deleted() {
                    return Err(Sent::Refused(CommentStoreError::Deleted));
                }
                after.body = Some(body.clone());
                after.edited = Some(self.at);
                CommentAction::Edited
            }
            Change::Delete => {
                if held.is_deleted() {
                    return Err(Sent::Done(held));
                }
                after.body = None;
                CommentAction::Deleted
            }
        };
        let head = read_head(self.table)
            .await
            .map_err(|()| Sent::Unavailable)?;
        let position = next_position(head).ok_or(Sent::Unavailable)?;
        let event = CommentEvent::of(&after, action, position, self.at);
        let next = Rev::fresh().ok_or(Sent::Unavailable)?;
        let transaction = items::change(self.table.name(), &after, &rev, &next, &event, head)
            .ok_or(Sent::Unavailable)?;
        Ok(BuiltChange {
            transaction,
            after,
            next,
        })
    }

    async fn send(&self, built: &BuiltChange, token: &Token) -> Sent<Comment, CommentStoreError> {
        match self
            .table
            .transact(&built.transaction, token, "comment change")
            .await
        {
            Ok(()) => Sent::Done(built.after.clone()),
            // The counter moved, or another change replaced the comment,
            // since the read; a fresh read sees the comment as it is.
            Err(Classified::Conflict { .. } | Classified::Retry) => Sent::Again,
            Err(Classified::Unknown) => Sent::Unknown,
            Err(Classified::Unavailable(_)) => Sent::Unavailable,
        }
    }

    /// The comment item carries the `rev` that only this change wrote.
    async fn settle(&self, built: &BuiltChange) -> Settled<Comment> {
        match read_comment(self.table, self.id).await {
            Ok(Some((held, rev))) if rev == built.next => Settled::Matches(held),
            Ok(_) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

fn answer<T>(done: Final<T, CommentStoreError>) -> Result<T, CommentStoreError> {
    match done {
        Final::Done(done) => Ok(done),
        Final::Refused(refused) => Err(refused),
        Final::Unavailable => Err(CommentStoreError::Unavailable),
    }
}

impl CommentStore for DynamoCommentStore {
    async fn add(&self, comment: NewComment) -> Result<Added, CommentStoreError> {
        let op = AddOp {
            table: &self.table,
            new: &comment,
            limit: self.limit,
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn get(&self, id: &CommentId) -> Result<Comment, CommentStoreError> {
        match read_comment(&self.table, id).await {
            Ok(Some((comment, _))) => Ok(comment),
            Ok(None) => Err(CommentStoreError::NotFound),
            Err(()) => Err(CommentStoreError::Unavailable),
        }
    }

    async fn of_flare(&self, flare: &ShareId) -> Result<Vec<Comment>, CommentStoreError> {
        let read = self
            .table
            .query(
                &comment_partition(flare),
                COMMENT_PREFIX,
                self.limit,
                None,
                ItemKind::Comment,
            )
            .await
            .map_err(|_| CommentStoreError::Unavailable)?;
        read.iter()
            .map(|item| comment_from(item).map(|(comment, _)| comment))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                malformed(error);
                CommentStoreError::Unavailable
            })
    }

    async fn edit(
        &self,
        id: &CommentId,
        body: CommentBody,
        at: Timestamp,
    ) -> Result<Comment, CommentStoreError> {
        let change = Change::Edit(body);
        let op = ChangeOp {
            table: &self.table,
            id,
            change: &change,
            at,
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn delete(&self, id: &CommentId, at: Timestamp) -> Result<Comment, CommentStoreError> {
        let change = Change::Delete;
        let op = ChangeOp {
            table: &self.table,
            id,
            change: &change,
            at,
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn events(
        &self,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> Result<Vec<CommentEvent>, CommentStoreError> {
        let Some(low) = after.get().checked_add(1) else {
            return Ok(Vec::new());
        };
        if low > upto.get() {
            return Ok(Vec::new());
        }
        let low = event_sort_key(Position::new(low));
        let high = event_sort_key(upto);
        let read = self
            .table
            .query_between(
                &comment_partition(flare),
                (&low, &high),
                limit,
                ItemKind::CommentEvent,
            )
            .await
            .map_err(|_| CommentStoreError::Unavailable)?;
        read.iter()
            .map(event_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                malformed(error);
                CommentStoreError::Unavailable
            })
    }
}

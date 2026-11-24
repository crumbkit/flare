//! [`DynamoCursorStore`], flare-core's `CursorStore` on the table.
//!
//! A cursor is one item under its person's partition, and the person's
//! count of cursors is one more item. A new cursor is one transaction
//! that puts the cursor if absent and raises the count below the limit,
//! and a move is one transaction that changes the position only from the
//! value that was read.

use crate::dynamodb::Table;
use crate::dynamodb::cancel::Classified;
use crate::dynamodb::items::{Item, ItemKind, Malformed, attr, keyed, number, read_number};
use crate::dynamodb::keys::Key;
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::dynamodb::settle::malformed;
use crate::dynamodb::transact::{
    ABSENT, ItemRole, Transaction, count, key_map, names, put, values,
};
use aws_sdk_dynamodb::types::{TransactWriteItem, Update};
use flare_core::{
    CommentStoreError, CursorName, CursorStore, MAX_CURSORS_PER_PERSON, Position, PrincipalId,
};

/// Keeps each person's named cursors in the table.
#[derive(Debug, Clone)]
pub struct DynamoCursorStore {
    table: Table,
    limit: usize,
}

impl DynamoCursorStore {
    pub(crate) fn from_table(table: Table) -> Self {
        Self {
            table,
            limit: MAX_CURSORS_PER_PERSON,
        }
    }

    /// The same store with `limit` cursors per person, so a test reaches
    /// the limit with a few cursors.
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }
}

fn position_of(item: &Item) -> Result<Position, Malformed> {
    read_number(item, attr::POSITION, ItemKind::Cursor).map(Position::new)
}

async fn read_cursor(
    table: &Table,
    owner: &PrincipalId,
    name: &CursorName,
) -> Result<Option<Position>, ()> {
    let item = table
        .get(&Key::cursor(owner, name), ItemKind::Cursor)
        .await
        .map_err(|_| ())?;
    item.as_ref()
        .map(position_of)
        .transpose()
        .map_err(malformed)
}

struct AdvanceOp<'a> {
    table: &'a Table,
    owner: &'a PrincipalId,
    name: &'a CursorName,
    to: Position,
    limit: usize,
}

impl Attempt for AdvanceOp<'_> {
    type Built = Transaction;
    type Done = Position;
    type Refused = CommentStoreError;

    async fn build(&self) -> Result<Transaction, Sent<Position, CommentStoreError>> {
        let held = read_cursor(self.table, self.owner, self.name)
            .await
            .map_err(|()| Sent::Unavailable)?;
        let table = self.table.name();
        let key = Key::cursor(self.owner, self.name);
        let mut transaction = Transaction::default();
        match held {
            Some(held) if held >= self.to => return Err(Sent::Done(held)),
            Some(held) => {
                let update = Update::builder()
                    .table_name(table)
                    .set_key(Some(key_map(&key)))
                    .update_expression("SET #p = :to")
                    .condition_expression("#p = :read")
                    .set_expression_attribute_names(Some(names(&[("#p", attr::POSITION)])))
                    .set_expression_attribute_values(Some(values(vec![
                        (":to", number(self.to.get())),
                        (":read", number(held.get())),
                    ])))
                    .build()
                    .map_err(|_| Sent::Unavailable)?;
                transaction.push(
                    ItemRole::CursorItem,
                    TransactWriteItem::builder().update(update).build(),
                );
            }
            None => {
                let count = count(table, &Key::cursor_count(self.owner), 1, self.limit)
                    .ok_or(Sent::Unavailable)?;
                transaction.push(ItemRole::CursorCount, count);
                let item = keyed(&key, [(attr::POSITION, number(self.to.get()))]);
                transaction.push(
                    ItemRole::CursorItem,
                    put(table, item, Some(ABSENT)).ok_or(Sent::Unavailable)?,
                );
            }
        }
        Ok(transaction)
    }

    async fn send(&self, built: &Transaction, token: &Token) -> Sent<Position, CommentStoreError> {
        match self.table.transact(built, token, "cursor").await {
            Ok(()) => Sent::Done(self.to),
            Err(Classified::Conflict {
                role: ItemRole::CursorCount,
                ..
            }) => Sent::Refused(CommentStoreError::TooMany),
            // The cursor was made or moved since the read; a fresh read
            // sees where it is.
            Err(Classified::Conflict { .. } | Classified::Retry) => Sent::Again,
            Err(Classified::Unknown) => Sent::Unknown,
            Err(Classified::Unavailable(_)) => Sent::Unavailable,
        }
    }

    /// A cursor at or above the target is either this write's result or a
    /// later move, and in both cases the cursor holds at least the target.
    async fn settle(&self, _built: &Transaction) -> Settled<Position> {
        match read_cursor(self.table, self.owner, self.name).await {
            Ok(Some(held)) if held >= self.to => Settled::Matches(held),
            Ok(_) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

impl CursorStore for DynamoCursorStore {
    async fn get(
        &self,
        owner: &PrincipalId,
        name: &CursorName,
    ) -> Result<Option<Position>, CommentStoreError> {
        read_cursor(&self.table, owner, name)
            .await
            .map_err(|()| CommentStoreError::Unavailable)
    }

    async fn advance(
        &self,
        owner: &PrincipalId,
        name: &CursorName,
        to: Position,
    ) -> Result<Position, CommentStoreError> {
        let op = AdvanceOp {
            table: &self.table,
            owner,
            name,
            to,
            limit: self.limit,
        };
        match run(self.table.policy(), &op).await {
            Final::Done(done) => Ok(done),
            Final::Refused(refused) => Err(refused),
            Final::Unavailable => Err(CommentStoreError::Unavailable),
        }
    }
}

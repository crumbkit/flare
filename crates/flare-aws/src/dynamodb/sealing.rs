//! [`DynamoSealingKeys`], flare-core's `SealingKeys` on the table.
//!
//! Key `n` is the item `V#<n, 10 digits>` under the partition `KEY#SIGNIN`,
//! written only if absent, so two processes that rotate together write one
//! key and the loser reads the winner's. The write of key `n + 1` also sets
//! the TTL attribute of key `n` to the end of its grace, so the table
//! deletes it some time after it stops being readable. A read compares the
//! next key's creation time with `now` itself, so a key that the TTL has
//! not deleted yet is still unreadable.

use crate::dynamodb::Table;
use crate::dynamodb::cancel::Classified;
use crate::dynamodb::items::{
    Item, ItemKind, Malformed, attr, keyed, number, read_number, read_text,
};
use crate::dynamodb::keys::{Key, SEALING_KEY_PREFIX, SEALING_PARTITION, sealing_key_of};
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::dynamodb::settle::malformed;
use crate::dynamodb::transact::{ABSENT, ItemRole, Transaction, key_map, names, put, values};
use aws_sdk_dynamodb::primitives::Blob;
use aws_sdk_dynamodb::types::{AttributeValue, TransactWriteItem, Update};
use flare_core::{
    KEY_GRACE, KeyBytes, KeyId, SealingKey, SealingKeys, SealingKeysError, Timestamp,
};

/// Keeps the sealing keys of the sign-in state in the table.
#[derive(Debug, Clone)]
pub struct DynamoSealingKeys {
    table: Table,
}

impl DynamoSealingKeys {
    pub(crate) fn from_table(table: Table) -> Self {
        Self { table }
    }
}

fn key_item(key: &SealingKey) -> Item {
    keyed(
        &Key::sealing_key(key.id),
        [
            (
                attr::BYTES,
                AttributeValue::B(Blob::new(key.bytes.expose().to_vec())),
            ),
            (attr::CREATED, number(key.created.as_secs())),
        ],
    )
}

fn key_from(item: &Item) -> Result<SealingKey, Malformed> {
    let kind = ItemKind::SealingKey;
    let id = sealing_key_of(read_text(item, attr::SK, kind)?).ok_or(Malformed(kind))?;
    let bytes = match item.get(attr::BYTES) {
        Some(AttributeValue::B(blob)) => {
            <[u8; 32]>::try_from(blob.as_ref()).map_err(|_| Malformed(kind))?
        }
        _ => return Err(Malformed(kind)),
    };
    let created = Timestamp::from_secs(read_number(item, attr::CREATED, kind)?);
    Ok(SealingKey::new(id, KeyBytes::new(bytes), created))
}

async fn read_key(table: &Table, id: KeyId) -> Result<Option<SealingKey>, ()> {
    let item = table
        .get(&Key::sealing_key(id), ItemKind::SealingKey)
        .await
        .map_err(|_| ())?;
    item.map(|item| key_from(&item))
        .transpose()
        .map_err(malformed)
}

async fn read_newest(table: &Table) -> Result<Option<SealingKey>, ()> {
    let item = table
        .query_last(SEALING_PARTITION, SEALING_KEY_PREFIX, ItemKind::SealingKey)
        .await
        .map_err(|_| ())?;
    item.map(|item| key_from(&item))
        .transpose()
        .map_err(malformed)
}

/// The transaction that writes `key` and, when `old` is given, ends the
/// grace of the key before it in the table's TTL.
fn write(table: &str, key: &SealingKey, old: Option<&SealingKey>) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::SealingKeyItem,
        put(table, key_item(key), Some(ABSENT))?,
    );
    if let Some(old) = old {
        let ends = key.created.as_secs().saturating_add(KEY_GRACE);
        let update = Update::builder()
            .table_name(table)
            .set_key(Some(key_map(&Key::sealing_key(old.id))))
            .update_expression("SET #t = :ends")
            .condition_expression("attribute_exists(pk)")
            .set_expression_attribute_names(Some(names(&[("#t", attr::TTL)])))
            .set_expression_attribute_values(Some(values(vec![(":ends", number(ends))])))
            .build()
            .ok()?;
        transaction.push(
            ItemRole::OldSealingKey,
            TransactWriteItem::builder().update(update).build(),
        );
    }
    Some(transaction)
}

/// A call that answers the newest key, and writes the first key or, when
/// `rotating`, the next key.
struct NewestOp<'a> {
    table: &'a Table,
    fresh: &'a KeyBytes,
    now: Timestamp,
    rotating: bool,
}

/// A built write: the transaction and the key it writes.
struct BuiltKey {
    transaction: Transaction,
    key: SealingKey,
}

impl Attempt for NewestOp<'_> {
    type Built = BuiltKey;
    type Done = SealingKey;
    type Refused = SealingKeysError;

    async fn build(&self) -> Result<BuiltKey, Sent<SealingKey, SealingKeysError>> {
        let newest = read_newest(self.table)
            .await
            .map_err(|()| Sent::Unavailable)?;
        let (id, old) = match newest {
            None => (KeyId::FIRST, None),
            Some(newest) if self.rotating && newest.is_due(self.now) => {
                let Some(id) = newest.id.next() else {
                    return Err(Sent::Done(newest));
                };
                (id, Some(newest))
            }
            Some(newest) => return Err(Sent::Done(newest)),
        };
        let key = SealingKey::new(id, self.fresh.clone(), self.now);
        let transaction = write(self.table.name(), &key, old.as_ref()).ok_or(Sent::Unavailable)?;
        Ok(BuiltKey { transaction, key })
    }

    async fn send(&self, built: &BuiltKey, token: &Token) -> Sent<SealingKey, SealingKeysError> {
        match self
            .table
            .transact(&built.transaction, token, "write sealing key")
            .await
        {
            Ok(()) => Sent::Done(built.key.clone()),
            // Another process wrote the key first, so the call reads again.
            Err(
                Classified::Conflict {
                    role: ItemRole::SealingKeyItem | ItemRole::OldSealingKey,
                    ..
                }
                | Classified::Retry,
            ) => Sent::Again,
            Err(Classified::Unknown) => Sent::Unknown,
            Err(_) => Sent::Unavailable,
        }
    }

    async fn settle(&self, built: &BuiltKey) -> Settled<SealingKey> {
        match read_key(self.table, built.key.id).await {
            Ok(Some(held)) if held.bytes == built.key.bytes => Settled::Matches(held),
            Ok(_) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

fn answer(answer: Final<SealingKey, SealingKeysError>) -> Result<SealingKey, SealingKeysError> {
    match answer {
        Final::Done(done) => Ok(done),
        Final::Refused(refused) => Err(refused),
        Final::Unavailable => Err(SealingKeysError::Unavailable),
    }
}

impl SealingKeys for DynamoSealingKeys {
    async fn current(
        &self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Result<SealingKey, SealingKeysError> {
        let op = NewestOp {
            table: &self.table,
            fresh: &fresh,
            now,
            rotating: false,
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn get(&self, id: KeyId, now: Timestamp) -> Result<Option<SealingKey>, SealingKeysError> {
        let unavailable = |()| SealingKeysError::Unavailable;
        let Some(key) = read_key(&self.table, id).await.map_err(unavailable)? else {
            return Ok(None);
        };
        let Some(next) = id.next() else {
            return Ok(Some(key));
        };
        let after = read_key(&self.table, next).await.map_err(unavailable)?;
        let readable = after
            .is_none_or(|after| now.as_secs() < after.created.as_secs().saturating_add(KEY_GRACE));
        Ok(readable.then_some(key))
    }

    async fn rotate(
        &self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Result<SealingKey, SealingKeysError> {
        let op = NewestOp {
            table: &self.table,
            fresh: &fresh,
            now,
            rotating: true,
        };
        answer(run(self.table.policy(), &op).await)
    }
}

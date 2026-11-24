//! The items of login codes, tokens and label slots, and the transactions
//! that write them.
//!
//! A code item holds the code's owner, port and expiry, and the expiry
//! again in the attribute that the table's TTL reads, so DynamoDB deletes
//! the item some time after it expires. A use writes `used_by`, a random
//! value, so a settling read can tell its own use from another. A token
//! item holds the digest of the token's secret as 32 bytes, its owner,
//! label, creation time, expiry and, once revoked, its revoked time. A
//! label slot names the latest token of one owner and label, so a new
//! token with that label finds the one to revoke.

use crate::dynamodb::items::{
    Item, ItemKind, Malformed, attr, keyed, number, read_number, read_text, text,
};
use crate::dynamodb::keys::Key;
use crate::dynamodb::transact::{ABSENT, ItemRole, Transaction, key_map, names, put, values};
use aws_sdk_dynamodb::primitives::Blob;
use aws_sdk_dynamodb::types::{AttributeValue, TransactWriteItem, Update};
use flare_core::{
    Digest, Email, LoginCode, NewToken, StoredToken, Timestamp, TokenId, TokenLabel, UsedCode,
};

/// A login code as the table holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeldCode {
    pub(crate) owner: Email,
    pub(crate) port: u16,
    pub(crate) expires: Timestamp,
    /// The `used_by` of the use that spent the code, or `None`.
    pub(crate) used_by: Option<String>,
}

impl HeldCode {
    /// Whether a use at `now` may spend the code.
    pub(crate) fn usable(&self, now: Timestamp) -> bool {
        self.used_by.is_none() && now < self.expires
    }

    /// What a successful use answers.
    pub(crate) fn used(&self) -> UsedCode {
        UsedCode::new(self.owner.clone(), self.port)
    }
}

fn read_email(item: &Item, name: &str, kind: ItemKind) -> Result<Email, Malformed> {
    Email::try_from(read_text(item, name, kind)?).map_err(|_| Malformed(kind))
}

fn read_optional_text(
    item: &Item,
    name: &str,
    kind: ItemKind,
) -> Result<Option<String>, Malformed> {
    match item.get(name) {
        None => Ok(None),
        Some(AttributeValue::S(value)) => Ok(Some(value.clone())),
        Some(_) => Err(Malformed(kind)),
    }
}

/// A code's item.
pub(crate) fn code_item(code: &LoginCode) -> Item {
    let expires = number(code.expires().as_secs());
    keyed(
        &Key::code(code.digest()),
        [
            (attr::OWNER, text(code.owner().as_str())),
            (attr::PORT, number(u64::from(code.port()))),
            (attr::EXPIRES, expires.clone()),
            (attr::TTL, expires),
        ],
    )
}

/// The code that a code item holds.
pub(crate) fn code_from(item: &Item) -> Result<HeldCode, Malformed> {
    let kind = ItemKind::Code;
    let port = u16::try_from(read_number(item, attr::PORT, kind)?).map_err(|_| Malformed(kind))?;
    Ok(HeldCode {
        owner: read_email(item, attr::OWNER, kind)?,
        port,
        expires: Timestamp::from_secs(read_number(item, attr::EXPIRES, kind)?),
        used_by: read_optional_text(item, attr::USED_BY, kind)?,
    })
}

/// A token's item, revoked or not.
pub(crate) fn token_item(token: &StoredToken) -> Item {
    let mut item = keyed(
        &Key::token(token.id()),
        [
            (
                attr::DIGEST,
                AttributeValue::B(Blob::new(token.digest().as_bytes().to_vec())),
            ),
            (attr::OWNER, text(token.owner().as_str())),
            (attr::LABEL, text(token.label().as_str())),
            (attr::CREATED, number(token.created().as_secs())),
            (attr::EXPIRES, number(token.expires().as_secs())),
        ],
    );
    if let Some(revoked) = token.revoked() {
        item.insert(attr::REVOKED.to_owned(), number(revoked.as_secs()));
    }
    item
}

/// The token `id` that a token item holds.
pub(crate) fn token_from(item: &Item, id: &TokenId) -> Result<StoredToken, Malformed> {
    let kind = ItemKind::Token;
    let digest = match item.get(attr::DIGEST) {
        Some(AttributeValue::B(blob)) => <[u8; 32]>::try_from(blob.as_ref())
            .map(Digest::from_bytes)
            .map_err(|_| Malformed(kind))?,
        _ => return Err(Malformed(kind)),
    };
    let label =
        TokenLabel::try_from(read_text(item, attr::LABEL, kind)?).map_err(|_| Malformed(kind))?;
    let new = NewToken::new(
        id.clone(),
        digest,
        read_email(item, attr::OWNER, kind)?,
        label,
        Timestamp::from_secs(read_number(item, attr::CREATED, kind)?),
    );
    let revoked = match item.get(attr::REVOKED) {
        None => None,
        Some(_) => Some(Timestamp::from_secs(read_number(
            item,
            attr::REVOKED,
            kind,
        )?)),
    };
    Ok(StoredToken::new(
        new,
        Timestamp::from_secs(read_number(item, attr::EXPIRES, kind)?),
        revoked,
    ))
}

/// The id of the token that a label slot names.
pub(crate) fn slot_token(item: &Item) -> Result<TokenId, Malformed> {
    let kind = ItemKind::LabelSlot;
    TokenId::try_from(read_text(item, attr::TOKEN, kind)?).map_err(|_| Malformed(kind))
}

/// An update of the item at `key`, under `condition`.
fn update(
    table: &str,
    key: &Key,
    expression: &str,
    condition: &str,
    name_pairs: &[(&str, &str)],
    value_pairs: Vec<(&str, AttributeValue)>,
) -> Option<TransactWriteItem> {
    let update = Update::builder()
        .table_name(table)
        .set_key(Some(key_map(key)))
        .update_expression(expression)
        .condition_expression(condition)
        .set_expression_attribute_names(Some(names(name_pairs)))
        .set_expression_attribute_values(Some(values(value_pairs)))
        .build()
        .ok()?;
    Some(TransactWriteItem::builder().update(update).build())
}

/// The transaction of an issue: the code's item, only if absent.
pub(crate) fn issue(table: &str, code: &LoginCode) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::CodeItem,
        put(table, code_item(code), Some(ABSENT))?,
    );
    Some(transaction)
}

/// The transaction of a use: `used_by` set to `mark`, only while the code
/// is unused and before its expiry at `now`.
pub(crate) fn spend(
    table: &str,
    digest: &Digest,
    mark: &str,
    now: Timestamp,
) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::CodeItem,
        update(
            table,
            &Key::code(digest),
            "SET #u = :mark",
            "attribute_exists(pk) AND attribute_not_exists(#u) AND #e > :now",
            &[("#u", attr::USED_BY), ("#e", attr::EXPIRES)],
            vec![(":mark", text(mark)), (":now", number(now.as_secs()))],
        )?,
    );
    Some(transaction)
}

/// The label slot that a create read: absent, or naming a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SlotRead {
    Absent,
    Names(TokenId),
}

/// The transaction of a create: the token's item, only if absent; the
/// label slot, moved to the new token only from what was read; and, when
/// `previous` names the owner's active token with the label, its revoke at
/// the new token's creation time, only while it is unrevoked.
pub(crate) fn create(
    table: &str,
    token: &NewToken,
    slot: &SlotRead,
    previous: Option<&TokenId>,
) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::TokenItem,
        put(table, token_item(&token.clone().stored()), Some(ABSENT))?,
    );
    let slot_key = Key::label_slot(token.owner(), token.label());
    let mut slot_item = key_map(&slot_key);
    slot_item.insert(attr::TOKEN.to_owned(), text(token.id().as_str()));
    let slot_put = match slot {
        SlotRead::Absent => put(table, slot_item, Some(ABSENT))?,
        SlotRead::Names(read) => {
            let put = aws_sdk_dynamodb::types::Put::builder()
                .table_name(table)
                .set_item(Some(slot_item))
                .condition_expression("#t = :read")
                .set_expression_attribute_names(Some(names(&[("#t", attr::TOKEN)])))
                .set_expression_attribute_values(Some(values(vec![(":read", text(read.as_str()))])))
                .build()
                .ok()?;
            TransactWriteItem::builder().put(put).build()
        }
    };
    transaction.push(ItemRole::LabelSlot, slot_put);
    if let Some(previous) = previous {
        transaction.push(
            ItemRole::OldToken,
            revoke_item(table, previous, None, token.created())?,
        );
    }
    Some(transaction)
}

/// The update that revokes token `id` at `at`, only while it exists and is
/// unrevoked, and, when `owner` is given, only while `owner` owns it.
fn revoke_item(
    table: &str,
    id: &TokenId,
    owner: Option<&Email>,
    at: Timestamp,
) -> Option<TransactWriteItem> {
    let mut condition = "attribute_exists(pk) AND attribute_not_exists(#r)".to_owned();
    let mut name_pairs = vec![("#r", attr::REVOKED)];
    let mut value_pairs = vec![(":at", number(at.as_secs()))];
    if let Some(owner) = owner {
        condition.push_str(" AND #o = :owner");
        name_pairs.push(("#o", attr::OWNER));
        value_pairs.push((":owner", text(owner.as_str())));
    }
    update(
        table,
        &Key::token(id),
        "SET #r = :at",
        &condition,
        &name_pairs,
        value_pairs,
    )
}

/// The transaction of a revoke of `owner`'s token `id` at `at`.
pub(crate) fn revoke(
    table: &str,
    id: &TokenId,
    owner: &Email,
    at: Timestamp,
) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::TokenItem,
        revoke_item(table, id, Some(owner), at)?,
    );
    Some(transaction)
}

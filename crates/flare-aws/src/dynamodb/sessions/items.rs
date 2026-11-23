//! The items of browser sessions, and the transactions that write them.
//!
//! A session item, under `SESS#<digest>`, holds the person, their verified
//! address, when the session was created and when it expires, and the
//! expiry again in the attribute that the table's TTL reads. A link item,
//! under `SESSOF#<person>`, sorts by the session's expiry, so a query of a
//! person's links lists their live sessions from the earliest to expire. A
//! guard item beside the links carries a random `rev` that every sign-in of
//! the person replaces from the `rev` it read, so two sign-ins of one
//! person cannot both stay below the limit.

use crate::dynamodb::items::{
    Item, ItemKind, Malformed, Rev, attr, keyed, number, read_number, read_text, text,
};
use crate::dynamodb::keys::{Key, link_of};
use crate::dynamodb::transact::{ABSENT, ItemRole, Transaction, key_map, names, put, values};
use aws_sdk_dynamodb::types::{AttributeValue, Delete, Put, TransactWriteItem, Update};
use flare_core::{Digest, Email, Principal, PrincipalId, Timestamp};

/// A session as the table holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HeldSession {
    pub(crate) principal: Principal,
    pub(crate) created: Timestamp,
    pub(crate) expires: Timestamp,
}

/// A session that a transaction ends: the session as it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ended {
    pub(crate) digest: Digest,
    pub(crate) person: PrincipalId,
    pub(crate) expires: Timestamp,
}

/// The guard's `rev` as it was read, or `None` for a person with none.
pub(crate) type GuardRead = Option<String>;

fn read_principal(item: &Item, kind: ItemKind) -> Result<Principal, Malformed> {
    let id = PrincipalId::try_from(read_text(item, attr::PRINCIPAL, kind)?)
        .map_err(|_| Malformed(kind))?;
    match item.get(attr::EMAIL) {
        None => Ok(Principal::new(id)),
        Some(_) => {
            let email = Email::try_from(read_text(item, attr::EMAIL, kind)?)
                .map_err(|_| Malformed(kind))?;
            Ok(Principal::with_email(id, email))
        }
    }
}

/// A session's item.
pub(crate) fn session_item(
    digest: &Digest,
    principal: &Principal,
    created: Timestamp,
    expires: Timestamp,
) -> Item {
    let mut item = keyed(
        &Key::session(digest),
        [
            (attr::PRINCIPAL, text(principal.id().as_str())),
            (attr::CREATED, number(created.as_secs())),
            (attr::EXPIRES, number(expires.as_secs())),
            (attr::TTL, number(expires.as_secs())),
        ],
    );
    if let Some(email) = principal.email() {
        item.insert(attr::EMAIL.to_owned(), text(email.as_str()));
    }
    item
}

/// The session that a session item holds.
pub(crate) fn session_from(item: &Item) -> Result<HeldSession, Malformed> {
    let kind = ItemKind::Session;
    Ok(HeldSession {
        principal: read_principal(item, kind)?,
        created: Timestamp::from_secs(read_number(item, attr::CREATED, kind)?),
        expires: Timestamp::from_secs(read_number(item, attr::EXPIRES, kind)?),
    })
}

/// The expiry and digest that each link item names, in the items' order.
pub(crate) fn links_of(items: &[Item]) -> Vec<(Timestamp, Digest)> {
    items
        .iter()
        .filter_map(|item| match item.get(attr::SK) {
            Some(AttributeValue::S(sort)) => link_of(sort),
            _ => None,
        })
        .collect()
}

/// The `rev` of a guard item.
pub(crate) fn guard_rev(item: &Item) -> Result<String, Malformed> {
    read_text(item, attr::REV, ItemKind::SessionGuard).map(str::to_owned)
}

/// The link from `person` to the session `digest` that expires at
/// `expires`.
fn link_item(person: &PrincipalId, expires: Timestamp, digest: &Digest) -> Item {
    keyed(
        &Key::session_link(person, expires, digest),
        [(attr::TTL, number(expires.as_secs()))],
    )
}

fn delete(
    table: &str,
    key: &Key,
    condition: Option<(&str, Timestamp)>,
) -> Option<TransactWriteItem> {
    let mut delete = Delete::builder()
        .table_name(table)
        .set_key(Some(key_map(key)));
    if let Some((expression, expires)) = condition {
        delete = delete
            .condition_expression(expression)
            .set_expression_attribute_names(Some(names(&[("#e", attr::EXPIRES)])))
            .set_expression_attribute_values(Some(values(vec![(
                ":read",
                number(expires.as_secs()),
            )])));
    }
    Some(
        TransactWriteItem::builder()
            .delete(delete.build().ok()?)
            .build(),
    )
}

/// Pushes the deletes that end `ended`: the session item, which must still
/// expire when it was read, and its link.
fn push_end(transaction: &mut Transaction, table: &str, ended: &Ended) -> Option<()> {
    transaction.push(
        ItemRole::OldSession,
        delete(
            table,
            &Key::session(&ended.digest),
            Some(("attribute_not_exists(pk) OR #e = :read", ended.expires)),
        )?,
    );
    transaction.push(
        ItemRole::SessionLink,
        delete(
            table,
            &Key::session_link(&ended.person, ended.expires, &ended.digest),
            None,
        )?,
    );
    Some(())
}

/// What a sign-in writes.
pub(crate) struct Creation<'a> {
    pub(crate) digest: &'a Digest,
    pub(crate) principal: &'a Principal,
    pub(crate) created: Timestamp,
    pub(crate) expires: Timestamp,
    pub(crate) guard: &'a GuardRead,
    pub(crate) rev: &'a Rev,
    pub(crate) ended: &'a [Ended],
}

/// The transaction of a sign-in: the session and its link, only if the
/// session is absent; the person's guard, only from the `rev` that was
/// read; and the end of each session in `ended`.
pub(crate) fn create(table: &str, creation: &Creation<'_>) -> Option<Transaction> {
    let person = creation.principal.id();
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::SessionItem,
        put(
            table,
            session_item(
                creation.digest,
                creation.principal,
                creation.created,
                creation.expires,
            ),
            Some(ABSENT),
        )?,
    );
    transaction.push(
        ItemRole::SessionLink,
        put(
            table,
            link_item(person, creation.expires, creation.digest),
            None,
        )?,
    );
    let guard = keyed(
        &Key::session_guard(person),
        [(attr::REV, text(creation.rev.as_str()))],
    );
    let guard_put = match creation.guard {
        None => put(table, guard, Some(ABSENT))?,
        Some(read) => {
            let put = Put::builder()
                .table_name(table)
                .set_item(Some(guard))
                .condition_expression("#r = :read")
                .set_expression_attribute_names(Some(names(&[("#r", attr::REV)])))
                .set_expression_attribute_values(Some(values(vec![(":read", text(read))])))
                .build()
                .ok()?;
            TransactWriteItem::builder().put(put).build()
        }
    };
    transaction.push(ItemRole::SessionGuard, guard_put);
    for ended in creation.ended {
        push_end(&mut transaction, table, ended)?;
    }
    Some(transaction)
}

/// The transaction of an end: the deletes of `ended`.
pub(crate) fn end(table: &str, ended: &Ended) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    push_end(&mut transaction, table, ended)?;
    Some(transaction)
}

/// The transaction that deletes the sessions at `links` and the links,
/// whatever they now hold.
pub(crate) fn end_all(
    table: &str,
    person: &PrincipalId,
    links: &[(Timestamp, Digest)],
) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    for (expires, digest) in links {
        transaction.push(
            ItemRole::OldSession,
            delete(table, &Key::session(digest), None)?,
        );
        transaction.push(
            ItemRole::SessionLink,
            delete(table, &Key::session_link(person, *expires, digest), None)?,
        );
    }
    Some(transaction)
}

/// The transaction of an extension from `held` to `next`: the session's
/// expiry, only from the expiry that was read, and the link under the new
/// expiry in place of the old.
pub(crate) fn extend(
    table: &str,
    digest: &Digest,
    held: &HeldSession,
    next: Timestamp,
) -> Option<Transaction> {
    let person = held.principal.id();
    let mut transaction = Transaction::default();
    let update = Update::builder()
        .table_name(table)
        .set_key(Some(key_map(&Key::session(digest))))
        .update_expression("SET #e = :next, #t = :next")
        .condition_expression("#e = :read")
        .set_expression_attribute_names(Some(names(&[("#e", attr::EXPIRES), ("#t", attr::TTL)])))
        .set_expression_attribute_values(Some(values(vec![
            (":next", number(next.as_secs())),
            (":read", number(held.expires.as_secs())),
        ])))
        .build()
        .ok()?;
    transaction.push(
        ItemRole::SessionItem,
        TransactWriteItem::builder().update(update).build(),
    );
    transaction.push(
        ItemRole::SessionLink,
        delete(
            table,
            &Key::session_link(person, held.expires, digest),
            None,
        )?,
    );
    transaction.push(
        ItemRole::SessionLink,
        put(table, link_item(person, next, digest), None)?,
    );
    Some(transaction)
}

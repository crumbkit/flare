//! The items of response links and of the position counter, and the
//! transaction that writes a link. Comments and cursors share the counter.
//!
//! One link is two items: the pair item under the response's partition,
//! keyed by the flare, which makes a repeat find the first link, and the
//! link item under the flare's partition, keyed by the position, which a
//! flare's list reads in position order. The counter holds the head. A
//! link's transaction moves the counter from the value it read and puts
//! both items, so a link is visible exactly when its position is the head.

use crate::dynamodb::items::{
    Item, ItemKind, Malformed, attr, keyed, number, read_number, read_text, text,
};
use crate::dynamodb::keys::{Key, PAIR_PREFIX};
use crate::dynamodb::transact::{ABSENT, ItemRole, Transaction, key_map, names, put, values};
use aws_sdk_dynamodb::types::{TransactWriteItem, Update};
use flare_core::{NewLink, Position, ResponseLink, ShareId, ShareVersion, Timestamp};

fn read_share(item: &Item, name: &str, kind: ItemKind) -> Result<ShareId, Malformed> {
    ShareId::try_from(read_text(item, name, kind)?).map_err(|_| Malformed(kind))
}

fn read_version(item: &Item, kind: ItemKind) -> Result<ShareVersion, Malformed> {
    u32::try_from(read_number(item, attr::VERSION, kind)?)
        .ok()
        .and_then(|number| ShareVersion::try_from(number).ok())
        .ok_or(Malformed(kind))
}

/// The attributes that both items of a link hold.
fn link_attributes(
    link: &ResponseLink,
) -> [(&'static str, aws_sdk_dynamodb::types::AttributeValue); 5] {
    [
        (attr::RESPONSE, text(link.response.as_str())),
        (attr::FLARE, text(link.flare.as_str())),
        (attr::VERSION, number(u64::from(link.version.get()))),
        (attr::CREATED, number(link.at.as_secs())),
        (attr::POSITION, number(link.position.get())),
    ]
}

/// The link that a pair item or a link item holds.
pub(crate) fn link_from(item: &Item) -> Result<ResponseLink, Malformed> {
    let kind = ItemKind::Link;
    let link = NewLink::new(
        read_share(item, attr::RESPONSE, kind)?,
        read_share(item, attr::FLARE, kind)?,
        read_version(item, kind)?,
        Timestamp::from_secs(read_number(item, attr::CREATED, kind)?),
    );
    Ok(link.at_position(Position::new(read_number(item, attr::POSITION, kind)?)))
}

/// The head that the counter item holds, or position 0 for no item.
pub(crate) fn head_from(item: Option<&Item>) -> Result<Position, Malformed> {
    item.map_or(Ok(Position::new(0)), |item| {
        read_number(item, attr::COUNT, ItemKind::Sequence).map(Position::new)
    })
}

/// The prefix of the pair items' sort keys.
pub(crate) const PAIRS: &str = PAIR_PREFIX;

/// The transaction of a link at the position after `head`: the counter
/// moves from `head`, and both items are put only if absent.
pub(crate) fn link(table: &str, link: &ResponseLink, head: Position) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::Sequence,
        sequence_update(table, head, link.position)?,
    );
    let pair = keyed(
        &Key::pair(&link.response, &link.flare),
        link_attributes(link),
    );
    transaction.push(ItemRole::PairItem, put(table, pair, Some(ABSENT))?);
    let listed = keyed(
        &Key::link(&link.flare, link.position),
        link_attributes(link),
    );
    transaction.push(ItemRole::LinkItem, put(table, listed, Some(ABSENT))?);
    Some(transaction)
}

/// The write that moves the position counter from `head` to `next`, only
/// if the counter still holds `head`. Every item that takes a position
/// carries it, so two of them cannot take the same one.
pub(crate) fn sequence_update(
    table: &str,
    head: Position,
    next: Position,
) -> Option<TransactWriteItem> {
    let (condition, read) = if head.get() == 0 {
        ("attribute_not_exists(#n)", None)
    } else {
        ("#n = :read", Some((":read", number(head.get()))))
    };
    let mut value_pairs = vec![(":next", number(next.get()))];
    value_pairs.extend(read);
    let counter = Update::builder()
        .table_name(table)
        .set_key(Some(key_map(&Key::sequence())))
        .update_expression("SET #n = :next")
        .condition_expression(condition)
        .set_expression_attribute_names(Some(names(&[("#n", attr::COUNT)])))
        .set_expression_attribute_values(Some(values(value_pairs)))
        .build()
        .ok()?;
    Some(TransactWriteItem::builder().update(counter).build())
}

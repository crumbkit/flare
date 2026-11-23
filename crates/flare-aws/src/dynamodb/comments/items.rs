//! The items of comments: the comment, the pointer from its id to it, its
//! events and the key slot, to and from DynamoDB's attribute maps, and the
//! transactions that write them.
//!
//! A comment lives under its flare's comment partition at the position of
//! its creation, so a flare's list is one query. The pointer item holds
//! the flare and position of an id, which never change. The comment item
//! carries a `rev` that every edit and delete replaces, so a change applies
//! only to the comment it read.

use crate::dynamodb::items::{
    Item, ItemKind, Malformed, Rev, attr, keyed, number, read_number, read_text, text,
};
use crate::dynamodb::keys::Key;
use crate::dynamodb::responses::items::sequence_update;
use crate::dynamodb::transact::{
    ABSENT, ItemRole, Transaction, count, key_map, names, put, values,
};
use aws_sdk_dynamodb::types::{AttributeValue, TransactWriteItem, Update};
use flare_core::{
    Comment, CommentAction, CommentBody, CommentEvent, CommentId, NewComment, Position,
    PrincipalId, ShareId, Timestamp,
};

fn optional<'a>(item: &'a Item, name: &str, kind: ItemKind) -> Result<Option<&'a str>, Malformed> {
    match item.get(name) {
        None => Ok(None),
        Some(AttributeValue::S(value)) => Ok(Some(value)),
        Some(_) => Err(Malformed(kind)),
    }
}

fn read_comment_id(item: &Item, name: &str, kind: ItemKind) -> Result<CommentId, Malformed> {
    CommentId::try_from(read_text(item, name, kind)?).map_err(|_| Malformed(kind))
}

fn read_flare(item: &Item, kind: ItemKind) -> Result<ShareId, Malformed> {
    ShareId::try_from(read_text(item, attr::FLARE, kind)?).map_err(|_| Malformed(kind))
}

fn read_author(item: &Item, kind: ItemKind) -> Result<PrincipalId, Malformed> {
    PrincipalId::try_from(read_text(item, attr::AUTHOR, kind)?).map_err(|_| Malformed(kind))
}

fn read_parent(item: &Item, kind: ItemKind) -> Result<Option<CommentId>, Malformed> {
    optional(item, attr::PARENT, kind)?
        .map(|text| CommentId::try_from(text).map_err(|_| Malformed(kind)))
        .transpose()
}

/// The attributes of a comment item.
fn comment_attributes(comment: &Comment, rev: &Rev) -> Vec<(&'static str, AttributeValue)> {
    let mut attributes = vec![
        (attr::COMMENT, text(comment.id.as_str())),
        (attr::FLARE, text(comment.flare.as_str())),
        (attr::AUTHOR, text(comment.author.as_str())),
        (attr::CREATED, number(comment.created.as_secs())),
        (attr::POSITION, number(comment.position.get())),
        (attr::REV, text(rev.as_str())),
    ];
    if let Some(parent) = &comment.parent {
        attributes.push((attr::PARENT, text(parent.as_str())));
    }
    if let Some(body) = &comment.body {
        attributes.push((attr::BODY, text(body.as_str())));
    }
    if let Some(edited) = comment.edited {
        attributes.push((attr::EDITED, number(edited.as_secs())));
    }
    attributes
}

/// The comment that an item holds, with its `rev`.
pub(crate) fn comment_from(item: &Item) -> Result<(Comment, Rev), Malformed> {
    let kind = ItemKind::Comment;
    let body = optional(item, attr::BODY, kind)?
        .map(|text| CommentBody::try_from(text).map_err(|_| Malformed(kind)))
        .transpose()?;
    let edited = match item.get(attr::EDITED) {
        None => None,
        Some(_) => Some(Timestamp::from_secs(read_number(item, attr::EDITED, kind)?)),
    };
    let comment = Comment::new(
        read_comment_id(item, attr::COMMENT, kind)?,
        read_flare(item, kind)?,
        read_parent(item, kind)?,
        read_author(item, kind)?,
        body,
        Timestamp::from_secs(read_number(item, attr::CREATED, kind)?),
        Position::new(read_number(item, attr::POSITION, kind)?),
    )
    .with_edited(edited);
    let rev = Rev::from_text(read_text(item, attr::REV, kind)?);
    Ok((comment, rev))
}

/// The flare and position that a pointer item holds.
pub(crate) fn pointer_from(item: &Item) -> Result<(ShareId, Position), Malformed> {
    let kind = ItemKind::CommentPointer;
    Ok((
        read_flare(item, kind)?,
        Position::new(read_number(item, attr::POSITION, kind)?),
    ))
}

/// The comment id that a key slot holds.
pub(crate) fn slot_from(item: &Item) -> Result<CommentId, Malformed> {
    read_comment_id(item, attr::COMMENT, ItemKind::CommentKeySlot)
}

fn action_name(action: CommentAction) -> Option<&'static str> {
    match action {
        CommentAction::Created => Some("created"),
        CommentAction::Edited => Some("edited"),
        CommentAction::Deleted => Some("deleted"),
        _ => None,
    }
}

/// The event that an item holds.
pub(crate) fn event_from(item: &Item) -> Result<CommentEvent, Malformed> {
    let kind = ItemKind::CommentEvent;
    let action = match read_text(item, attr::ACTION, kind)? {
        "created" => CommentAction::Created,
        "edited" => CommentAction::Edited,
        "deleted" => CommentAction::Deleted,
        _ => return Err(Malformed(kind)),
    };
    Ok(CommentEvent::new(
        Position::new(read_number(item, attr::POSITION, kind)?),
        action,
        read_comment_id(item, attr::COMMENT, kind)?,
        read_flare(item, kind)?,
        read_parent(item, kind)?,
        read_author(item, kind)?,
        Timestamp::from_secs(read_number(item, attr::CREATED, kind)?),
    ))
}

fn event_item(event: &CommentEvent) -> Option<Item> {
    let mut attributes = vec![
        (attr::ACTION, text(action_name(event.action)?)),
        (attr::COMMENT, text(event.comment.as_str())),
        (attr::FLARE, text(event.flare.as_str())),
        (attr::AUTHOR, text(event.author.as_str())),
        (attr::CREATED, number(event.at.as_secs())),
        (attr::POSITION, number(event.position.get())),
    ];
    if let Some(parent) = &event.parent {
        attributes.push((attr::PARENT, text(parent.as_str())));
    }
    Some(keyed(
        &Key::comment_event(&event.flare, event.position),
        attributes,
    ))
}

/// The transaction of a new comment at the position after `head`: the
/// counter moves from `head`; the comment, its pointer, its event and the
/// author's key slot are put only if absent; and the flare's count rises
/// only below `limit`.
pub(crate) fn add(
    table: &str,
    new: &NewComment,
    comment: &Comment,
    rev: &Rev,
    head: Position,
    limit: usize,
) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::CommentCount,
        count(table, &Key::comment_count(&comment.flare), 1, limit)?,
    );
    transaction.push(
        ItemRole::Sequence,
        sequence_update(table, head, comment.position)?,
    );
    let item = keyed(
        &Key::comment(&comment.flare, comment.position),
        comment_attributes(comment, rev),
    );
    transaction.push(ItemRole::CommentItem, put(table, item, Some(ABSENT))?);
    let pointer = keyed(
        &Key::comment_pointer(&comment.id),
        [
            (attr::FLARE, text(comment.flare.as_str())),
            (attr::POSITION, number(comment.position.get())),
        ],
    );
    transaction.push(ItemRole::CommentPointer, put(table, pointer, Some(ABSENT))?);
    let slot = keyed(
        &Key::comment_key(&comment.flare, &new.author, &new.key),
        [
            (attr::COMMENT, text(comment.id.as_str())),
            (attr::POSITION, number(comment.position.get())),
        ],
    );
    transaction.push(ItemRole::CommentKeySlot, put(table, slot, Some(ABSENT))?);
    let event = CommentEvent::of(comment, CommentAction::Created, comment.position, new.at);
    transaction.push(
        ItemRole::CommentEvent,
        put(table, event_item(&event)?, Some(ABSENT))?,
    );
    Some(transaction)
}

/// The transaction of an edit or a delete that takes the position `event`
/// holds after `head`: the counter moves from `head`, the comment item
/// changes only if its `rev` is still `read`, and the event is put.
pub(crate) fn change(
    table: &str,
    after: &Comment,
    read: &Rev,
    next: &Rev,
    event: &CommentEvent,
    head: Position,
) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::Sequence,
        sequence_update(table, head, event.position)?,
    );
    let (expression, name_pairs, value_pairs) = match (&after.body, after.edited) {
        (Some(body), Some(edited)) => (
            "SET #rev = :next, #body = :body, #edited = :edited",
            vec![
                ("#rev", attr::REV),
                ("#body", attr::BODY),
                ("#edited", attr::EDITED),
            ],
            vec![
                (":next", text(next.as_str())),
                (":read", text(read.as_str())),
                (":body", text(body.as_str())),
                (":edited", number(edited.as_secs())),
            ],
        ),
        _ => (
            "SET #rev = :next REMOVE #body",
            vec![("#rev", attr::REV), ("#body", attr::BODY)],
            vec![
                (":next", text(next.as_str())),
                (":read", text(read.as_str())),
            ],
        ),
    };
    let comment = Update::builder()
        .table_name(table)
        .set_key(Some(key_map(&Key::comment(&after.flare, after.position))))
        .update_expression(expression)
        .condition_expression("#rev = :read")
        .set_expression_attribute_names(Some(names(&name_pairs)))
        .set_expression_attribute_values(Some(values(value_pairs)))
        .build()
        .ok()?;
    transaction.push(
        ItemRole::CommentItem,
        TransactWriteItem::builder().update(comment).build(),
    );
    transaction.push(
        ItemRole::CommentEvent,
        put(table, event_item(event)?, Some(ABSENT))?,
    );
    Some(transaction)
}

/// The key slot's key for `new`, which the store reads before it writes.
pub(crate) fn slot_key(new: &NewComment) -> Key {
    Key::comment_key(&new.flare, &new.author, &new.key)
}

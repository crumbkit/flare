//! The pieces that the transactions share: a key as a map, the placeholder
//! maps of an expression, a conditional put and a bounded count.

use crate::dynamodb::items::{Item, attr, number, text};
use crate::dynamodb::keys::Key;
use aws_sdk_dynamodb::types::{Put, TransactWriteItem, Update};
use std::collections::HashMap;

const BELOW_MAX: &str = "attribute_not_exists(#n) OR #n < :max";

pub(crate) fn key_map(key: &Key) -> Item {
    HashMap::from([
        (attr::PK.to_owned(), text(&key.pk)),
        (attr::SK.to_owned(), text(&key.sk)),
    ])
}

pub(crate) fn names(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(placeholder, name)| ((*placeholder).to_owned(), (*name).to_owned()))
        .collect()
}

pub(crate) fn values(pairs: Vec<(&str, aws_sdk_dynamodb::types::AttributeValue)>) -> Item {
    pairs
        .into_iter()
        .map(|(placeholder, value)| (placeholder.to_owned(), value))
        .collect()
}

pub(crate) fn put(table: &str, item: Item, condition: Option<&str>) -> Option<TransactWriteItem> {
    let put = Put::builder()
        .table_name(table)
        .set_item(Some(item))
        .set_condition_expression(condition.map(str::to_owned))
        .build()
        .ok()?;
    Some(TransactWriteItem::builder().put(put).build())
}

/// Adds `delta` to the count item at `key`. A positive delta applies only
/// below `max`.
pub(crate) fn count(table: &str, key: &Key, delta: i64, max: usize) -> Option<TransactWriteItem> {
    let mut update = Update::builder()
        .table_name(table)
        .set_key(Some(key_map(key)))
        .update_expression("ADD #n :delta")
        .expression_attribute_names("#n", attr::COUNT);
    update = if delta > 0 {
        update
            .condition_expression(BELOW_MAX)
            .set_expression_attribute_values(Some(values(vec![
                (
                    ":delta",
                    aws_sdk_dynamodb::types::AttributeValue::N(delta.to_string()),
                ),
                (":max", number(u64::try_from(max).ok()?)),
            ])))
    } else {
        update.set_expression_attribute_values(Some(values(vec![(
            ":delta",
            aws_sdk_dynamodb::types::AttributeValue::N(delta.to_string()),
        )])))
    };
    Some(
        TransactWriteItem::builder()
            .update(update.build().ok()?)
            .build(),
    )
}

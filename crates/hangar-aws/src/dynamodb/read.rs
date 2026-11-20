//! The table's reads: a consistent `GetItem` of one key and a consistent
//! `Query` of one prefix of one partition, with a bound on the items read.
//!
//! A failed read is [`Unavailable`], never an absent item or an empty list,
//! and its log line names the call, the item kind and the error code.

use crate::dynamodb::items::{Item, ItemKind, attr, text};
use crate::dynamodb::keys::Key;
use crate::dynamodb::{Table, error_code};
use aws_sdk_dynamodb::operation::RequestId;
use aws_sdk_dynamodb::operation::get_item::builders::GetItemFluentBuilder;
use aws_sdk_dynamodb::operation::query::builders::QueryFluentBuilder;
use std::collections::HashMap;

/// A read failed, so the store cannot answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Unavailable;

fn log_failure(call: &'static str, kind: ItemKind, code: &str, request_id: Option<&str>) {
    tracing::warn!(
        call,
        item = kind.name(),
        code,
        request_id = request_id.unwrap_or("none"),
        "a DynamoDB read failed"
    );
}

impl Table {
    /// The consistent `GetItem` of `key`.
    pub(crate) fn get_request(&self, key: &Key) -> GetItemFluentBuilder {
        self.client()
            .get_item()
            .table_name(self.name())
            .key(attr::PK, text(&key.pk))
            .key(attr::SK, text(&key.sk))
            .consistent_read(true)
    }

    /// The consistent `Query` of the sort keys in `partition` that start
    /// with `prefix`, ascending, at most `limit` items per page.
    pub(crate) fn query_request(
        &self,
        partition: &str,
        prefix: &str,
        limit: i32,
    ) -> QueryFluentBuilder {
        self.client()
            .query()
            .table_name(self.name())
            .key_condition_expression("pk = :pk AND begins_with(sk, :prefix)")
            .expression_attribute_values(":pk", text(partition))
            .expression_attribute_values(":prefix", text(prefix))
            .consistent_read(true)
            .scan_index_forward(true)
            .limit(limit)
    }

    /// The item at `key`, or `None` when the table has none.
    pub(crate) async fn get(&self, key: &Key, kind: ItemKind) -> Result<Option<Item>, Unavailable> {
        match self.get_request(key).send().await {
            Ok(output) => Ok(output.item),
            Err(error) => {
                log_failure("GetItem", kind, &error_code(&error), error.request_id());
                Err(Unavailable)
            }
        }
    }

    /// The first `limit` items of `partition` whose sort keys start with
    /// `prefix`, in sort key order. The query follows the pages until it
    /// has `limit` items or the partition ends.
    pub(crate) async fn query(
        &self,
        partition: &str,
        prefix: &str,
        limit: usize,
        kind: ItemKind,
    ) -> Result<Vec<Item>, Unavailable> {
        let mut items = Vec::new();
        let mut start: Option<HashMap<String, aws_sdk_dynamodb::types::AttributeValue>> = None;
        loop {
            let wanted =
                i32::try_from(limit.saturating_sub(items.len())).map_err(|_| Unavailable)?;
            let page = self
                .query_request(partition, prefix, wanted)
                .set_exclusive_start_key(start.take())
                .send()
                .await;
            let page = match page {
                Ok(page) => page,
                Err(error) => {
                    log_failure("Query", kind, &error_code(&error), error.request_id());
                    return Err(Unavailable);
                }
            };
            items.extend(page.items.unwrap_or_default());
            match page.last_evaluated_key {
                Some(next) if items.len() < limit => start = Some(next),
                _ => return Ok(items),
            }
        }
    }
}

#[cfg(test)]
mod tests;

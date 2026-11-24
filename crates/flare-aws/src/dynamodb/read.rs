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
    /// with `prefix`, ascending, at most `limit` items per page, after
    /// `start` when given. DynamoDB does not require `start` to name an
    /// item that exists, so a query after a deleted item starts at the
    /// next one.
    pub(crate) fn query_request(
        &self,
        partition: &str,
        prefix: &str,
        limit: i32,
        start: Option<&Key>,
    ) -> QueryFluentBuilder {
        let start = start.map(|key| {
            HashMap::from([
                (attr::PK.to_owned(), text(&key.pk)),
                (attr::SK.to_owned(), text(&key.sk)),
            ])
        });
        self.client()
            .query()
            .table_name(self.name())
            .key_condition_expression("pk = :pk AND begins_with(sk, :prefix)")
            .expression_attribute_values(":pk", text(partition))
            .expression_attribute_values(":prefix", text(prefix))
            .consistent_read(true)
            .scan_index_forward(true)
            .limit(limit)
            .set_exclusive_start_key(start)
    }

    /// The consistent `Query` of the sort keys in `partition` from `low`
    /// to `high`, both included, ascending, at most `limit` items.
    pub(crate) async fn query_between(
        &self,
        partition: &str,
        (low, high): (&str, &str),
        limit: usize,
        kind: ItemKind,
    ) -> Result<Vec<Item>, Unavailable> {
        let request = |wanted| {
            self.client()
                .query()
                .table_name(self.name())
                .key_condition_expression("pk = :pk AND sk BETWEEN :low AND :high")
                .expression_attribute_values(":pk", text(partition))
                .expression_attribute_values(":low", text(low))
                .expression_attribute_values(":high", text(high))
                .consistent_read(true)
                .scan_index_forward(true)
                .limit(wanted)
        };
        self.pages(request, limit, kind).await
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
    /// `prefix` and sort after `start`, or from the first without it, in
    /// sort key order.
    pub(crate) async fn query(
        &self,
        partition: &str,
        prefix: &str,
        limit: usize,
        start: Option<&Key>,
        kind: ItemKind,
    ) -> Result<Vec<Item>, Unavailable> {
        let request = |wanted| self.query_request(partition, prefix, wanted, start);
        self.pages(request, limit, kind).await
    }

    /// The first `limit` items that the query `request` builds, which
    /// follows the pages until it has `limit` items or the query ends. A
    /// limit of 0 reads nothing.
    async fn pages(
        &self,
        request: impl Fn(i32) -> QueryFluentBuilder,
        limit: usize,
        kind: ItemKind,
    ) -> Result<Vec<Item>, Unavailable> {
        // DynamoDB refuses a `Limit` of 0.
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut items = Vec::new();
        let mut start: Option<HashMap<String, aws_sdk_dynamodb::types::AttributeValue>> = None;
        loop {
            let wanted =
                i32::try_from(limit.saturating_sub(items.len())).map_err(|_| Unavailable)?;
            // The first page starts where `request` says; each later page
            // starts after the last key of the page before it.
            let mut builder = request(wanted);
            if let Some(next) = start.take() {
                builder = builder.set_exclusive_start_key(Some(next));
            }
            let page = builder.send().await;
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

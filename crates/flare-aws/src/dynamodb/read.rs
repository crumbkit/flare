//! The table's reads: a consistent `GetItem` of one key, a consistent
//! `BatchGetItem` of many keys, and a consistent `Query` of one prefix of
//! one partition, with a bound on the items read.
//!
//! A failed read is [`Unavailable`], never an absent item or an empty list,
//! and its log line names the call, the item kind and the error code.

use crate::dynamodb::items::{Item, ItemKind, attr, text};
use crate::dynamodb::keys::Key;
use crate::dynamodb::transact::key_map;
use crate::dynamodb::{Table, error_code};
use aws_sdk_dynamodb::operation::RequestId;
use aws_sdk_dynamodb::operation::get_item::builders::GetItemFluentBuilder;
use aws_sdk_dynamodb::operation::query::builders::QueryFluentBuilder;
use aws_sdk_dynamodb::types::KeysAndAttributes;
use std::collections::HashMap;

/// The most keys one `BatchGetItem` names, which is DynamoDB's own limit.
const KEYS_PER_BATCH: usize = 100;

/// How many times a batch asks again for the keys DynamoDB left
/// unprocessed before the read fails as unavailable.
const UNPROCESSED_ROUNDS: usize = 5;

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

    /// The items at `keys` that the table holds, in no set order, read in
    /// consistent batches of [`KEYS_PER_BATCH`].
    ///
    /// DynamoDB can leave keys unprocessed when it is short of capacity.
    /// This read asks again for them at once, up to
    /// [`UNPROCESSED_ROUNDS`] times, without the backoff that the write
    /// loop uses, because only retry.rs may wait; a batch still short after
    /// that answers [`Unavailable`], and the caller's repeat reads again.
    pub(crate) async fn get_many(
        &self,
        keys: &[Key],
        kind: ItemKind,
    ) -> Result<Vec<Item>, Unavailable> {
        let mut items = Vec::new();
        for batch in keys.chunks(KEYS_PER_BATCH) {
            let mut wanted = KeysAndAttributes::builder()
                .set_keys(Some(batch.iter().map(key_map).collect()))
                .consistent_read(true)
                .build()
                .map_err(|_| Unavailable)?;
            let mut rounds = 0;
            loop {
                let answer = self
                    .client()
                    .batch_get_item()
                    .request_items(self.name(), wanted)
                    .send()
                    .await;
                let answer = match answer {
                    Ok(answer) => answer,
                    Err(error) => {
                        log_failure(
                            "BatchGetItem",
                            kind,
                            &error_code(&error),
                            error.request_id(),
                        );
                        return Err(Unavailable);
                    }
                };
                if let Some(mut found) = answer.responses {
                    items.extend(found.remove(self.name()).unwrap_or_default());
                }
                match answer
                    .unprocessed_keys
                    .and_then(|mut left| left.remove(self.name()))
                {
                    Some(left) if !left.keys().is_empty() => wanted = left,
                    _ => break,
                }
                rounds += 1;
                if rounds == UNPROCESSED_ROUNDS {
                    log_failure("BatchGetItem", kind, "UnprocessedKeys", None);
                    return Err(Unavailable);
                }
            }
        }
        Ok(items)
    }

    /// The first `limit` items of `partition` whose sort keys start with
    /// `prefix`, in sort key order.
    pub(crate) async fn query(
        &self,
        partition: &str,
        prefix: &str,
        limit: usize,
        kind: ItemKind,
    ) -> Result<Vec<Item>, Unavailable> {
        let request = |wanted| self.query_request(partition, prefix, wanted);
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
            let page = request(wanted)
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

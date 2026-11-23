//! The transactions of the four writes. Each write is one
//! `TransactWriteItems`, so its conditions and its changes apply together
//! or not at all.
//!
//! A grant or a revoke under an admin checks the admin's Admin grant in the
//! same transaction as the write. When the admin acts on their own grant,
//! the check joins that item's own condition, because a transaction names
//! each item at most once.

use crate::dynamodb::items::{
    Item, Rev, attr, grant_item, grantee_entry_item, number, owner_entry_item, role_name,
    share_item, text, version_item,
};
use crate::dynamodb::keys::Key;
use aws_sdk_dynamodb::types::{
    ConditionCheck, Delete, Put, ReturnValuesOnConditionCheckFailure, TransactWriteItem, Update,
};
use flare_core::{
    Grant, Granter, MAX_GRANTS, MAX_SHARES_PER_OWNER, MAX_VERSIONS, PrincipalId, Revoker, Role,
    Share, ShareId, SharedVersion, Timestamp,
};
use std::collections::HashMap;

/// What one item of a transaction is for, so a failed condition can be
/// read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemRole {
    ShareItem,
    Version,
    OwnerEntry,
    ShareCount,
    AdminCheck,
    GrantItem,
    GranteeEntry,
    GrantCount,
}

/// The items of one transaction, with the role of each.
#[derive(Debug, Clone, Default)]
pub(crate) struct Transaction {
    pub(crate) items: Vec<TransactWriteItem>,
    pub(crate) roles: Vec<ItemRole>,
}

impl Transaction {
    fn push(&mut self, role: ItemRole, item: TransactWriteItem) {
        self.items.push(item);
        self.roles.push(role);
    }
}

/// A grant to write, read from a `GrantWrite` and the grant the store read.
#[derive(Debug)]
pub(crate) struct GrantPlan<'a> {
    pub(crate) share: &'a ShareId,
    pub(crate) grantee: &'a PrincipalId,
    pub(crate) role: Role,
    pub(crate) by: &'a Granter,
    /// The time of first grant: the read grant's, or the write's for a new
    /// grantee.
    pub(crate) created: Timestamp,
    /// The read grant's `rev`, or `None` when the grantee held no grant.
    pub(crate) read: Option<&'a Rev>,
    /// The new `rev`.
    pub(crate) rev: &'a Rev,
}

/// A grant to end, read from a `GrantRevoke` and the grant the store read.
#[derive(Debug)]
pub(crate) struct RevokePlan<'a> {
    pub(crate) share: &'a ShareId,
    pub(crate) grantee: &'a PrincipalId,
    pub(crate) by: &'a Revoker,
    /// The read grant's time of first grant, which its entry's key holds.
    pub(crate) created: Timestamp,
    /// The read grant's `rev`.
    pub(crate) read: &'a Rev,
}

const ABSENT: &str = "attribute_not_exists(pk)";
const PRESENT: &str = "attribute_exists(pk)";
const SAME_REV: &str = "#rev = :read_rev";
const IS_ADMIN: &str = "#r = :admin";
const BELOW_MAX: &str = "attribute_not_exists(#n) OR #n < :max";

fn key_map(key: &Key) -> Item {
    HashMap::from([
        (attr::PK.to_owned(), text(&key.pk)),
        (attr::SK.to_owned(), text(&key.sk)),
    ])
}

fn names(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(placeholder, name)| ((*placeholder).to_owned(), (*name).to_owned()))
        .collect()
}

fn values(pairs: Vec<(&str, aws_sdk_dynamodb::types::AttributeValue)>) -> Item {
    pairs
        .into_iter()
        .map(|(placeholder, value)| (placeholder.to_owned(), value))
        .collect()
}

fn put(table: &str, item: Item, condition: Option<&str>) -> Option<TransactWriteItem> {
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
fn count(table: &str, key: &Key, delta: i64, max: usize) -> Option<TransactWriteItem> {
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

/// Checks that `admin` holds an Admin grant on `share`.
fn admin_check(table: &str, share: &ShareId, admin: &PrincipalId) -> Option<TransactWriteItem> {
    let check = ConditionCheck::builder()
        .table_name(table)
        .set_key(Some(key_map(&Key::grant(share, admin))))
        .condition_expression(IS_ADMIN)
        .set_expression_attribute_names(Some(names(&[("#r", attr::ROLE)])))
        .set_expression_attribute_values(Some(values(vec![(
            ":admin",
            text(role_name(Role::Admin)),
        )])))
        .build()
        .ok()?;
    Some(TransactWriteItem::builder().condition_check(check).build())
}

/// The condition on a grant item, its names and its values: the `rev` that
/// was read, or the item's absence, and the admin's role when the admin
/// acts on their own grant.
fn grant_condition(read: Option<&Rev>, folded: bool) -> (String, HashMap<String, String>, Item) {
    let mut condition = Vec::new();
    let mut name_pairs = Vec::new();
    let mut value_pairs = Vec::new();
    if let Some(read) = read {
        condition.push(SAME_REV);
        name_pairs.push(("#rev", attr::REV));
        value_pairs.push((":read_rev", text(read.as_str())));
    } else {
        condition.push(ABSENT);
    }
    if folded {
        condition.push(IS_ADMIN);
        name_pairs.push(("#r", attr::ROLE));
        value_pairs.push((":admin", text(role_name(Role::Admin))));
    }
    (
        condition.join(" AND "),
        names(&name_pairs),
        values(value_pairs),
    )
}

fn nonempty<K, V>(map: HashMap<K, V>) -> Option<HashMap<K, V>> {
    (!map.is_empty()).then_some(map)
}

/// The transaction of a create: the share, its version 1 and its owner
/// entry, each only if absent, and the owner's share count, only below
/// [`MAX_SHARES_PER_OWNER`].
pub(crate) fn create(table: &str, share: &Share, first: &SharedVersion) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::ShareItem,
        put(table, share_item(share)?, Some(ABSENT))?,
    );
    transaction.push(
        ItemRole::Version,
        put(table, version_item(first)?, Some(ABSENT))?,
    );
    transaction.push(
        ItemRole::OwnerEntry,
        put(table, owner_entry_item(share)?, Some(ABSENT))?,
    );
    let share_count = Key::share_count(share.owner());
    transaction.push(
        ItemRole::ShareCount,
        count(table, &share_count, 1, MAX_SHARES_PER_OWNER)?,
    );
    Some(transaction)
}

/// Sets `latest` on the item at `key`, under `condition`.
fn set_latest(
    table: &str,
    key: &Key,
    condition: &str,
    condition_values: Vec<(&str, aws_sdk_dynamodb::types::AttributeValue)>,
    next: u32,
) -> Option<TransactWriteItem> {
    let mut all_values = condition_values;
    all_values.push((":next", number(u64::from(next))));
    let update = Update::builder()
        .table_name(table)
        .set_key(Some(key_map(key)))
        .update_expression("SET #l = :next")
        .condition_expression(condition)
        .expression_attribute_names("#l", attr::LATEST)
        .set_expression_attribute_values(Some(values(all_values)))
        .build()
        .ok()?;
    Some(TransactWriteItem::builder().update(update).build())
}

/// The transaction of a publish of `version` to `share` as it was read:
/// the share's `latest` moves to the version's number only from the number
/// that was read and below [`MAX_VERSIONS`], the version is put only if
/// absent, and the owner entry's `latest` moves with it.
pub(crate) fn publish(table: &str, share: &Share, version: &SharedVersion) -> Option<Transaction> {
    let next = version.number().get();
    let read = u64::from(share.latest().get());
    let mut transaction = Transaction::default();
    transaction.push(
        ItemRole::ShareItem,
        set_latest(
            table,
            &Key::share(share.id()),
            "#l = :read AND #l < :max",
            vec![
                (":read", number(read)),
                (":max", number(u64::from(MAX_VERSIONS))),
            ],
            next,
        )?,
    );
    transaction.push(
        ItemRole::Version,
        put(table, version_item(version)?, Some(ABSENT))?,
    );
    let entry = Key::owner_entry(share.owner(), share.created(), share.id());
    transaction.push(
        ItemRole::OwnerEntry,
        set_latest(table, &entry, PRESENT, Vec::new(), next)?,
    );
    Some(transaction)
}

/// The admin whose grant the transaction checks with a separate item, and
/// whether the check joins the grant item's own condition instead.
fn authority(admin: Option<&PrincipalId>, grantee: &PrincipalId) -> (Option<PrincipalId>, bool) {
    match admin {
        Some(admin) if admin == grantee => (None, true),
        Some(admin) => (Some(admin.clone()), false),
        None => (None, false),
    }
}

/// The transaction of a grant: the admin check, the grant item from the
/// `rev` that was read with a new `rev`, the grantee's entry and, for a new
/// grantee, the share's grant count, only below [`MAX_GRANTS`].
pub(crate) fn grant(table: &str, plan: &GrantPlan<'_>) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    let (check, folded) = authority(plan.by.admin(), plan.grantee);
    if let Some(admin) = check {
        transaction.push(
            ItemRole::AdminCheck,
            admin_check(table, plan.share, &admin)?,
        );
    }
    let grant = Grant::new(
        plan.share.clone(),
        plan.grantee.clone(),
        plan.role,
        plan.by.principal().clone(),
        plan.created,
    );
    let (condition, condition_names, condition_values) = grant_condition(plan.read, folded);
    let item = Put::builder()
        .table_name(table)
        .set_item(Some(grant_item(&grant, plan.rev)))
        .condition_expression(condition)
        .set_expression_attribute_names(nonempty(condition_names))
        .set_expression_attribute_values(nonempty(condition_values))
        .return_values_on_condition_check_failure(ReturnValuesOnConditionCheckFailure::AllOld)
        .build()
        .ok()?;
    transaction.push(
        ItemRole::GrantItem,
        TransactWriteItem::builder().put(item).build(),
    );
    transaction.push(
        ItemRole::GranteeEntry,
        put(table, grantee_entry_item(&grant), None)?,
    );
    if plan.read.is_none() {
        let grant_count = Key::grant_count(plan.share);
        transaction.push(
            ItemRole::GrantCount,
            count(table, &grant_count, 1, MAX_GRANTS)?,
        );
    }
    Some(transaction)
}

fn delete(table: &str, key: &Key) -> Option<Delete> {
    Delete::builder()
        .table_name(table)
        .set_key(Some(key_map(key)))
        .build()
        .ok()
}

/// The transaction of a revoke: the admin check, the delete of the grant
/// item at the `rev` that was read, the delete of the grantee's entry, and
/// one less in the share's grant count.
pub(crate) fn revoke(table: &str, plan: &RevokePlan<'_>) -> Option<Transaction> {
    let mut transaction = Transaction::default();
    let (check, folded) = authority(plan.by.admin(), plan.grantee);
    if let Some(admin) = check {
        transaction.push(
            ItemRole::AdminCheck,
            admin_check(table, plan.share, &admin)?,
        );
    }
    let (condition, condition_names, condition_values) = grant_condition(Some(plan.read), folded);
    let item = Delete::builder()
        .table_name(table)
        .set_key(Some(key_map(&Key::grant(plan.share, plan.grantee))))
        .condition_expression(condition)
        .set_expression_attribute_names(nonempty(condition_names))
        .set_expression_attribute_values(nonempty(condition_values))
        .return_values_on_condition_check_failure(ReturnValuesOnConditionCheckFailure::AllOld)
        .build()
        .ok()?;
    transaction.push(
        ItemRole::GrantItem,
        TransactWriteItem::builder().delete(item).build(),
    );
    let entry = Key::grantee_entry(plan.grantee, plan.created, plan.share);
    transaction.push(
        ItemRole::GranteeEntry,
        TransactWriteItem::builder()
            .delete(delete(table, &entry)?)
            .build(),
    );
    let grant_count = Key::grant_count(plan.share);
    transaction.push(
        ItemRole::GrantCount,
        count(table, &grant_count, -1, MAX_GRANTS)?,
    );
    Some(transaction)
}

#[cfg(test)]
mod tests;

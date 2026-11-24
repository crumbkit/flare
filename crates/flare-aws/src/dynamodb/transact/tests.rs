use super::{GrantPlan, ItemRole, RevokePlan, Transaction, create, grant, publish, revoke};
use crate::dynamodb::items::Rev;
use aws_sdk_dynamodb::types::{AttributeValue, TransactWriteItem};
use flare_core::{
    DocumentText, Granter, PrincipalId, Revoker, Role, Share, ShareContent, ShareId, ShareKind,
    ShareTitle, ShareVersion, SharedVersion, Timestamp,
};
use std::collections::{HashMap, HashSet};

const TABLE: &str = "flare-test";

fn person(text: &str) -> PrincipalId {
    PrincipalId::try_from(text).unwrap()
}

fn share_id() -> ShareId {
    ShareId::from_random([5; 16])
}

fn rev(byte: u8) -> Rev {
    Rev::from_bytes([byte; 16])
}

/// The key, the condition and the values of one item of a transaction.
struct Op {
    kind: &'static str,
    key: (String, String),
    condition: Option<String>,
    values: HashMap<String, AttributeValue>,
}

fn text(map: &HashMap<String, AttributeValue>, name: &str) -> String {
    match map.get(name) {
        Some(AttributeValue::S(value)) => value.clone(),
        other => panic!("{name} is {other:?}"),
    }
}

fn key_of(map: &HashMap<String, AttributeValue>) -> (String, String) {
    (text(map, "pk"), text(map, "sk"))
}

fn op(item: &TransactWriteItem) -> Op {
    if let Some(check) = item.condition_check() {
        return Op {
            kind: "check",
            key: key_of(check.key()),
            condition: Some(check.condition_expression().to_owned()),
            values: check
                .expression_attribute_values()
                .cloned()
                .unwrap_or_default(),
        };
    }
    if let Some(put) = item.put() {
        return Op {
            kind: "put",
            key: key_of(put.item()),
            condition: put.condition_expression().map(str::to_owned),
            values: put
                .expression_attribute_values()
                .cloned()
                .unwrap_or_default(),
        };
    }
    if let Some(update) = item.update() {
        return Op {
            kind: "update",
            key: key_of(update.key()),
            condition: update.condition_expression().map(str::to_owned),
            values: update
                .expression_attribute_values()
                .cloned()
                .unwrap_or_default(),
        };
    }
    let delete = item
        .delete()
        .expect("an item is a check, a put, an update or a delete");
    Op {
        kind: "delete",
        key: key_of(delete.key()),
        condition: delete.condition_expression().map(str::to_owned),
        values: delete
            .expression_attribute_values()
            .cloned()
            .unwrap_or_default(),
    }
}

fn ops(transaction: &Transaction) -> Vec<Op> {
    assert_eq!(transaction.items.len(), transaction.roles.len());
    let ops: Vec<Op> = transaction.items.iter().map(op).collect();
    let keys: HashSet<&(String, String)> = ops.iter().map(|op| &op.key).collect();
    assert_eq!(
        keys.len(),
        ops.len(),
        "an item appears twice in one transaction"
    );
    ops
}

fn grant_key(grantee: &str) -> (String, String) {
    (format!("SHARE#{}", share_id()), format!("G#{grantee}"))
}

fn admin_value(op: &Op) -> bool {
    op.values
        .values()
        .any(|value| *value == AttributeValue::S("admin".to_owned()))
}

fn grant_plan<'a>(
    by: &'a Granter,
    read: Option<&'a Rev>,
    new: &'a Rev,
    id: &'a ShareId,
    grantee: &'a PrincipalId,
) -> GrantPlan<'a> {
    GrantPlan {
        share: id,
        grantee,
        role: Role::Reader,
        by,
        created: Timestamp::from_secs(10),
        read,
        rev: new,
        audience: None,
    }
}

#[test]
fn a_grant_by_an_admin_checks_the_admins_grant_in_the_same_transaction_as_the_write() {
    let (id, carol, admin) = (share_id(), person("carol@x.com"), person("admin@x.com"));
    let by = Granter::Admin(admin);
    let new = rev(2);
    let transaction = grant(TABLE, &grant_plan(&by, None, &new, &id, &carol)).unwrap();
    let ops = ops(&transaction);
    assert_eq!(transaction.roles[0], ItemRole::AdminCheck);
    assert_eq!(ops[0].kind, "check");
    assert_eq!(ops[0].key, grant_key("admin@x.com"));
    assert!(ops[0].condition.as_deref().unwrap().contains("= :admin"));
    assert!(admin_value(&ops[0]));
    assert_eq!(transaction.roles[1], ItemRole::GrantItem);
    assert_eq!(
        (ops[1].kind, &ops[1].key),
        ("put", &grant_key("carol@x.com"))
    );
    assert_eq!(
        ops[1].condition.as_deref(),
        Some("attribute_not_exists(pk)")
    );
    assert_eq!(transaction.roles[2], ItemRole::GranteeEntry);
    assert_eq!(transaction.roles[3], ItemRole::GrantCount);
    assert_eq!(ops.len(), 4);
}

#[test]
fn a_grant_by_the_owner_builds_no_admin_check() {
    let (id, carol) = (share_id(), person("carol@x.com"));
    let by = Granter::Owner(person("owner@x.com"));
    let (read, new) = (rev(1), rev(2));
    let transaction = grant(TABLE, &grant_plan(&by, Some(&read), &new, &id, &carol)).unwrap();
    let ops = ops(&transaction);
    assert!(ops.iter().all(|op| op.kind != "check"));
    assert!(!transaction.roles.contains(&ItemRole::AdminCheck));
    let put = &ops[0];
    assert_eq!(put.condition.as_deref(), Some("#rev = :read_rev"));
    assert_eq!(
        put.values.get(":read_rev"),
        Some(&AttributeValue::S(read.as_str().to_owned()))
    );
    // An existing grantee does not change the count.
    assert_eq!(ops.len(), 2);
}

#[test]
fn an_admin_who_changes_their_own_grant_builds_one_condition_on_that_item() {
    let (id, admin) = (share_id(), person("admin@x.com"));
    let by = Granter::Admin(admin.clone());
    let (read, new) = (rev(1), rev(2));
    let transaction = grant(TABLE, &grant_plan(&by, Some(&read), &new, &id, &admin)).unwrap();
    let ops = ops(&transaction);
    assert!(ops.iter().all(|op| op.kind != "check"));
    let own: Vec<&Op> = ops
        .iter()
        .filter(|op| op.key == grant_key("admin@x.com"))
        .collect();
    assert_eq!(own.len(), 1);
    let condition = own[0].condition.as_deref().unwrap();
    assert!(condition.contains("#rev = :read_rev") && condition.contains("= :admin"));
    assert!(admin_value(own[0]));
}

#[test]
fn a_revoke_by_an_admin_checks_the_admins_grant_in_the_same_transaction() {
    let (id, carol) = (share_id(), person("carol@x.com"));
    let by = Revoker::Admin(person("admin@x.com"));
    let read = rev(1);
    let plan = RevokePlan {
        share: &id,
        grantee: &carol,
        by: &by,
        created: Timestamp::from_secs(10),
        read: &read,
    };
    let transaction = revoke(TABLE, &plan).unwrap();
    let ops = ops(&transaction);
    assert_eq!(
        transaction.roles,
        [
            ItemRole::AdminCheck,
            ItemRole::GrantItem,
            ItemRole::GranteeEntry,
            ItemRole::GrantCount
        ]
    );
    assert_eq!(
        (ops[0].kind, &ops[0].key),
        ("check", &grant_key("admin@x.com"))
    );
    assert_eq!(
        (ops[1].kind, &ops[1].key),
        ("delete", &grant_key("carol@x.com"))
    );
    assert_eq!(ops[1].condition.as_deref(), Some("#rev = :read_rev"));
    assert_eq!(ops[2].kind, "delete");
    assert_eq!(ops[3].kind, "update");
}

#[test]
fn a_revoke_by_the_owner_or_the_grantee_builds_no_admin_check() {
    let (id, carol) = (share_id(), person("carol@x.com"));
    let read = rev(1);
    for by in [Revoker::Owner(person("owner@x.com")), Revoker::Grantee] {
        let plan = RevokePlan {
            share: &id,
            grantee: &carol,
            by: &by,
            created: Timestamp::from_secs(10),
            read: &read,
        };
        let transaction = revoke(TABLE, &plan).unwrap();
        assert!(ops(&transaction).iter().all(|op| op.kind != "check"));
        assert_eq!(transaction.roles.len(), 3);
    }
}

#[test]
fn an_admin_who_revokes_their_own_grant_builds_one_condition_on_that_item() {
    let (id, admin) = (share_id(), person("admin@x.com"));
    let by = Revoker::Admin(admin.clone());
    let read = rev(1);
    let plan = RevokePlan {
        share: &id,
        grantee: &admin,
        by: &by,
        created: Timestamp::from_secs(10),
        read: &read,
    };
    let transaction = revoke(TABLE, &plan).unwrap();
    let ops = ops(&transaction);
    assert!(ops.iter().all(|op| op.kind != "check"));
    let condition = ops[0].condition.as_deref().unwrap();
    assert!(condition.contains("#rev = :read_rev") && condition.contains("= :admin"));
}

fn share(latest: u32) -> Share {
    Share::new(
        share_id(),
        person("owner@x.com"),
        ShareKind::Document,
        ShareTitle::try_from("Notes").unwrap(),
        Timestamp::from_secs(10),
        ShareVersion::try_from(latest).unwrap(),
    )
}

fn version(number: u32) -> SharedVersion {
    SharedVersion::new(
        share_id(),
        ShareVersion::try_from(number).unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        person("owner@x.com"),
        Timestamp::from_secs(20),
    )
}

#[test]
fn a_create_writes_the_share_its_first_version_its_entry_and_the_count_together() {
    let transaction = create(TABLE, &share(1), &version(1)).unwrap();
    let ops = ops(&transaction);
    assert_eq!(
        transaction.roles,
        [
            ItemRole::ShareItem,
            ItemRole::Version,
            ItemRole::OwnerEntry,
            ItemRole::ShareCount
        ]
    );
    for put in &ops[..3] {
        assert_eq!(put.condition.as_deref(), Some("attribute_not_exists(pk)"));
    }
    assert_eq!(
        ops[3].condition.as_deref(),
        Some("attribute_not_exists(#n) OR #n < :max")
    );
    assert_eq!(
        ops[3].values.get(":max"),
        Some(&AttributeValue::N("500".to_owned()))
    );
}

#[test]
fn a_publish_moves_latest_only_from_the_number_it_read() {
    let transaction = publish(TABLE, &share(4), &version(5)).unwrap();
    let ops = ops(&transaction);
    assert_eq!(
        transaction.roles,
        [ItemRole::ShareItem, ItemRole::Version, ItemRole::OwnerEntry]
    );
    assert_eq!(
        ops[0].condition.as_deref(),
        Some("#l = :read AND #l < :max")
    );
    assert_eq!(
        ops[0].values.get(":read"),
        Some(&AttributeValue::N("4".to_owned()))
    );
    assert_eq!(
        ops[0].values.get(":max"),
        Some(&AttributeValue::N("100".to_owned()))
    );
    assert_eq!(ops[1].key.1, "V#0000000005");
    assert_eq!(ops[2].condition.as_deref(), Some("attribute_exists(pk)"));
}

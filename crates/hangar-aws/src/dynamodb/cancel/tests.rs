use super::{Classified, Failure, Reason, classify};
use crate::dynamodb::transact::ItemRole;
use aws_sdk_dynamodb::types::AttributeValue;
use std::collections::HashMap;

const GRANT_ROLES: [ItemRole; 4] = [
    ItemRole::AdminCheck,
    ItemRole::GrantItem,
    ItemRole::GranteeEntry,
    ItemRole::GrantCount,
];

fn reason(code: &str) -> Reason {
    Reason {
        code: Some(code.to_owned()),
        old: None,
    }
}

fn failed_with_old(role: &str) -> Reason {
    let item = HashMap::from([("role".to_owned(), AttributeValue::S(role.to_owned()))]);
    Reason {
        code: Some("ConditionalCheckFailed".to_owned()),
        old: Some(item),
    }
}

fn cancelled(codes: [&str; 4]) -> Failure {
    Failure::Cancelled(codes.map(reason).to_vec())
}

#[test]
fn a_failed_admin_check_comes_before_a_failed_grant_item_and_count() {
    let failure = cancelled([
        "ConditionalCheckFailed",
        "ConditionalCheckFailed",
        "None",
        "ConditionalCheckFailed",
    ]);
    assert!(matches!(
        classify(&GRANT_ROLES, failure),
        Classified::Conflict {
            role: ItemRole::AdminCheck,
            ..
        }
    ));
}

#[test]
fn a_failed_grant_item_comes_before_a_failed_count_and_keeps_its_old_item() {
    let failure = Failure::Cancelled(vec![
        reason("None"),
        failed_with_old("admin"),
        reason("None"),
        reason("ConditionalCheckFailed"),
    ]);
    let Classified::Conflict { role, old } = classify(&GRANT_ROLES, failure) else {
        panic!("a failed grant item is a conflict");
    };
    assert_eq!(role, ItemRole::GrantItem);
    assert_eq!(
        old.and_then(|item| item.get("role").cloned()),
        Some(AttributeValue::S("admin".to_owned()))
    );
}

#[test]
fn a_failed_count_alone_is_a_conflict_on_the_count() {
    let failure = cancelled(["None", "None", "None", "ConditionalCheckFailed"]);
    assert!(matches!(
        classify(&GRANT_ROLES, failure),
        Classified::Conflict {
            role: ItemRole::GrantCount,
            old: None
        }
    ));
}

#[test]
fn a_condition_failure_comes_before_a_transaction_conflict() {
    let failure = cancelled([
        "TransactionConflict",
        "None",
        "None",
        "ConditionalCheckFailed",
    ]);
    assert!(matches!(
        classify(&GRANT_ROLES, failure),
        Classified::Conflict {
            role: ItemRole::GrantCount,
            ..
        }
    ));
}

#[test]
fn conflicts_and_throttles_are_retried() {
    for code in [
        "TransactionConflict",
        "ThrottlingError",
        "ProvisionedThroughputExceeded",
    ] {
        let failure = cancelled(["None", code, "None", "None"]);
        assert_eq!(classify(&GRANT_ROLES, failure), Classified::Retry);
    }
    assert_eq!(
        classify(&GRANT_ROLES, Failure::Throttled),
        Classified::Retry
    );
}

#[test]
fn a_transaction_in_progress_or_a_lost_answer_is_an_unknown_outcome() {
    assert_eq!(
        classify(&GRANT_ROLES, Failure::InProgress),
        Classified::Unknown
    );
    assert_eq!(classify(&GRANT_ROLES, Failure::Lost), Classified::Unknown);
}

#[test]
fn any_other_reason_is_unavailable_and_names_its_code() {
    let failure = cancelled(["None", "ValidationError", "None", "None"]);
    assert_eq!(
        classify(&GRANT_ROLES, failure),
        Classified::Unavailable("ValidationError".to_owned())
    );
    assert_eq!(
        classify(
            &GRANT_ROLES,
            Failure::Refused("ResourceNotFoundException".to_owned())
        ),
        Classified::Unavailable("ResourceNotFoundException".to_owned())
    );
}

#[test]
fn reasons_that_do_not_match_the_items_are_unavailable() {
    let failure = Failure::Cancelled(vec![reason("ConditionalCheckFailed")]);
    assert!(matches!(
        classify(&GRANT_ROLES, failure),
        Classified::Unavailable(_)
    ));
}

#[test]
fn a_cancellation_with_no_failed_reason_is_unavailable() {
    let failure = cancelled(["None", "None", "None", "None"]);
    assert!(matches!(
        classify(&GRANT_ROLES, failure),
        Classified::Unavailable(_)
    ));
}

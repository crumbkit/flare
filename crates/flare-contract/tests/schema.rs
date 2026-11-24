//! The committed JSON Schema in `schema/` matches the contract's types, so
//! CI fails when a request, response or problem type changes without it.
#![cfg(test)]

use flare_contract::schema;

const COMMITTED: &str = include_str!("../../../schema/flare-api.schema.json");

#[test]
fn the_committed_schema_matches_the_contracts_types() {
    if let Err(drift) = schema::check(COMMITTED) {
        panic!("{drift}");
    }
}

#[test]
fn the_schema_check_fails_when_the_committed_schema_drifts() {
    let drifted = COMMITTED.replacen("\"title\"", "\"heading\"", 1);
    assert_ne!(drifted, COMMITTED);
    let drift = schema::check(&drifted).unwrap_err();
    assert!(
        drift
            .to_string()
            .contains("cargo run -p flare-contract --example schema")
    );
}

#[test]
fn the_schema_names_every_request_response_and_problem_type() {
    let generated: serde_json::Value = serde_json::from_str(&schema::generate()).unwrap();
    let defs = generated["$defs"].as_object().unwrap();
    for name in [
        "ShareRequest",
        "VersionRequest",
        "GrantRequest",
        "Shared",
        "ShareSummary",
        "ShareList",
        "OpenedShare",
        "ActivityPage",
        "ActivityItem",
        "ExchangeRequest",
        "TokenIssued",
        "ProblemDocument",
        "RevokeRequest",
        "Revoked",
        "GrantList",
        "GrantEntry",
        "TeamRequest",
        "MembersRequest",
        "TeamSummary",
        "TeamList",
        "TeamShown",
        "MembersChanged",
        "TeamDeleted",
    ] {
        assert!(defs.contains_key(name), "{name} is missing");
    }
}

#[test]
fn a_share_list_leaves_next_out_when_no_entry_follows_and_names_it_when_one_does() {
    let last = flare_contract::ShareList {
        items: Vec::new(),
        more: false,
        next: None,
    };
    let value = serde_json::to_value(&last).unwrap();
    assert_eq!(value, serde_json::json!({"items": [], "more": false}));
    let first = flare_contract::ShareList {
        items: Vec::new(),
        more: true,
        next: Some("fs1.cursor".to_owned()),
    };
    let value = serde_json::to_value(&first).unwrap();
    assert_eq!(value["next"], "fs1.cursor");
    let read: flare_contract::ShareList =
        serde_json::from_value(serde_json::json!({"items": [], "more": false})).unwrap();
    assert_eq!(read.next, None);
}

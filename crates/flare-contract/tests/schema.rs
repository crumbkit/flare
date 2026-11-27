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
        "TransferRequest",
        "Comment",
        "Reply",
        "CommentThread",
        "CommentOrReply",
        "NewComment",
        "CommentEdit",
        "NamedCursor",
    ] {
        assert!(defs.contains_key(name), "{name} is missing");
    }
}

#[test]
fn a_reply_has_a_parent_and_no_replies_so_a_second_level_cannot_be_written() {
    let generated: serde_json::Value = serde_json::from_str(&schema::generate()).unwrap();
    let defs = &generated["$defs"];
    let reply = defs["Reply"]["properties"].as_object().unwrap();
    assert!(reply.contains_key("parent"));
    assert!(!reply.contains_key("replies"));
    let comment = defs["Comment"]["properties"].as_object().unwrap();
    assert!(comment.contains_key("replies"));
    assert!(!comment.contains_key("parent"));
}

#[test]
fn the_activity_item_kinds_are_response_comment_reply_edited_and_deleted() {
    let generated: serde_json::Value = serde_json::from_str(&schema::generate()).unwrap();
    let kinds: Vec<&str> = generated["$defs"]["ActivityItem"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|variant| variant["properties"]["kind"]["const"].as_str())
        .collect();
    assert_eq!(kinds, ["response", "comment", "reply", "edited", "deleted"]);
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

#[test]
fn a_directory_team_summary_has_no_owner_or_created_time_and_says_who_keeps_it() {
    use flare_contract::{TeamManagedBy, TeamRole, TeamSummary};
    let directory = TeamSummary {
        name: "design".to_owned(),
        owner: None,
        managed_by: TeamManagedBy::Directory,
        role: TeamRole::Member,
        members: 2,
        created: None,
    };
    let written = serde_json::to_value(&directory).unwrap();
    assert_eq!(
        written,
        serde_json::json!({
            "name": "design",
            "managed_by": "directory",
            "role": "member",
            "members": 2,
        })
    );
    assert_eq!(
        serde_json::from_value::<TeamSummary>(written).unwrap(),
        directory
    );
}

#[test]
fn a_team_summary_from_a_server_that_predates_managed_by_reads_as_a_flare_team() {
    use flare_contract::{TeamManagedBy, TeamSummary};
    let older = serde_json::json!({
        "name": "design",
        "owner": "ana@example.com",
        "role": "owner",
        "members": 1,
        "created": "2026-11-23T10:00:00Z",
    });
    let read: TeamSummary = serde_json::from_value(older).unwrap();
    assert_eq!(read.managed_by, TeamManagedBy::Flare);
    assert_eq!(read.owner.as_deref(), Some("ana@example.com"));
}

//! `flare teams`, `flare grant --everyone`, `flare revoke`, `flare grants`
//! and `flare shares --everyone` against a fake share API: what each sends
//! and what it prints.
#![cfg(test)]

#[path = "support/api.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the fake API is test support"
)]
pub mod api;
#[path = "../../flare-auth-loopback/tests/support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch directory is flare-auth-loopback's test support, and these tests use part of it"
)]
pub mod loopback_scratch;
#[path = "support/run.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the run helper is test support, and each test file uses part of it"
)]
pub mod run;

use api::{FLARE, FakeApi, Fakes as _, problem};
use run::Home;
use serde_json::{Value, json};

fn logged_in(api: &FakeApi) -> Home {
    let home = Home::new();
    api.issue();
    home.sharing(&["login", &api.origin]).assert_ok();
    home
}

fn team(members: u32) -> Value {
    json!({
        "name": "design",
        "owner": "ana@example.org",
        "role": "owner",
        "members": members,
        "created": "2026-11-23T10:00:00Z",
    })
}

#[test]
fn teams_create_posts_the_name_and_prints_the_team() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route("POST", "/api/teams", 201, &team(1));
    let ran = home.sharing(&["teams", "create", "design", "--json"]);
    ran.assert_ok();
    assert!(ran.stdout.contains("design"), "{}", ran.all());
    let sent = &api.requests_to("/api/teams")[0];
    assert_eq!(sent.json(), json!({"name": "design"}));
}

#[test]
fn teams_create_keeps_the_servers_code_for_a_taken_name() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "POST",
        "/api/teams",
        409,
        &problem("flare::teams::name_taken", 409, &api.origin),
    );
    home.sharing(&["teams", "create", "design"])
        .assert_failed("flare::teams::name_taken");
}

#[test]
fn teams_add_and_remove_send_each_person_to_the_members_route() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let changed = json!({"team": team(2), "changed": ["jo@example.org"]});
    api.route("POST", "/api/teams/design/members", 200, &changed);
    api.route("DELETE", "/api/teams/design/members", 200, &changed);
    home.sharing(&["teams", "add", "design", "--with", "jo@example.org"])
        .assert_ok();
    home.sharing(&[
        "teams",
        "remove",
        "design",
        "--with",
        "jo@example.org",
        "--yes",
    ])
    .assert_ok();
    let sent = api.requests_to("/api/teams/design/members");
    assert_eq!(sent[0].method, "POST");
    assert_eq!(sent[1].method, "DELETE");
    assert_eq!(sent[1].json(), json!({"with": ["jo@example.org"]}));
}

#[test]
fn teams_list_and_show_print_the_teams_and_the_members() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route("GET", "/api/teams", 200, &json!({"items": [team(2)]}));
    api.route(
        "GET",
        "/api/teams/design",
        200,
        &json!({"team": team(2), "members": ["ana@example.org", "jo@example.org"]}),
    );
    let listed = home.sharing(&["teams", "list"]);
    listed.assert_ok();
    assert!(listed.stdout.contains("design"), "{}", listed.all());
    let shown = home.sharing(&["teams", "show", "design"]);
    assert!(shown.stdout.contains("design"), "{}", shown.all());
    shown.assert_ok();
    assert!(shown.stdout.contains("jo@example.org"), "{}", shown.all());
}

#[test]
fn teams_delete_says_the_grants_stay_dormant_and_the_name_is_never_reused() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let deleted = json!({"team": "design", "grants_dormant": true, "name_reusable": false});
    api.route("DELETE", "/api/teams/design", 200, &deleted);
    let ran = home.sharing(&["teams", "delete", "design", "--yes"]);
    ran.assert_ok();
    let text = ran.all();
    assert!(
        text.contains("stay on each flare and open nothing"),
        "{text}"
    );
    assert!(text.contains("never used again"), "{text}");
    api.route("DELETE", "/api/teams/design", 200, &deleted);
    let json = home.sharing(&["teams", "delete", "design", "--yes", "--json"]);
    json.assert_ok();
    assert!(
        json.stdout.contains("\"grants_dormant\":true"),
        "{}",
        json.all()
    );
    assert!(
        json.stdout.contains("\"name_reusable\":false"),
        "{}",
        json.all()
    );
}

#[test]
fn teams_delete_without_yes_and_without_input_asks_and_sends_nothing() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let ran = home.sharing(&["teams", "delete", "design", "--no-input"]);
    assert_ne!(ran.code, Some(0), "{}", ran.all());
    assert!(api.requests_to("/api/teams/design").is_empty());
}

#[test]
fn revoke_and_teams_remove_without_yes_and_without_input_send_nothing() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let ran = home.sharing(&["revoke", FLARE, "--everyone", "--no-input"]);
    assert_ne!(ran.code, Some(0), "{}", ran.all());
    let ran = home.sharing(&[
        "teams",
        "remove",
        "design",
        "--with",
        "jo@example.org",
        "--no-input",
    ]);
    assert_ne!(ran.code, Some(0), "{}", ran.all());
    assert!(api.requests_to(&format!("/api/grants/{FLARE}")).is_empty());
    assert!(api.requests_to("/api/teams/design/members").is_empty());
}

#[test]
fn grant_everyone_sends_everyone_and_a_team_goes_in_with() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "POST",
        &format!("/api/grants/{FLARE}"),
        200,
        &json!({
            "share": {
                "id": FLARE, "title": "Plans", "format": "markdown",
                "owner": "ana@example.org", "role": "owner", "version": 1,
                "created": "2026-11-23T10:00:00Z",
            },
            "granted": ["team:design", "everyone"],
            "responds_to": [],
        }),
    );
    home.sharing(&["grant", FLARE, "--with", "team:design", "--everyone"])
        .assert_ok();
    let sent = &api.requests_to(&format!("/api/grants/{FLARE}"))[0];
    assert_eq!(
        sent.json(),
        json!({"with": ["team:design"], "everyone": true, "re": []})
    );
}

#[test]
fn revoke_sends_people_teams_and_everyone_to_the_grants_route() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let path = format!("/api/grants/{FLARE}");
    api.route(
        "DELETE",
        &path,
        200,
        &json!({"share": FLARE, "revoked": ["jo@example.org", "everyone"]}),
    );
    let ran = home.sharing(&[
        "revoke",
        FLARE,
        "--with",
        "jo@example.org",
        "--everyone",
        "--yes",
    ]);
    ran.assert_ok();
    assert!(ran.stdout.contains("everyone"), "{}", ran.all());
    let sent = &api.requests_to(&path)[0];
    assert_eq!(sent.method, "DELETE");
    assert_eq!(
        sent.json(),
        json!({"with": ["jo@example.org"], "everyone": true})
    );
}

#[test]
fn revoke_with_nobody_named_is_refused_before_any_request_and_one_name_is_admitted() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let refused = home.sharing(&["revoke", FLARE]);
    assert_ne!(refused.code, Some(0), "{}", refused.all());
    assert!(api.requests_to(&format!("/api/grants/{FLARE}")).is_empty());
    api.route(
        "DELETE",
        &format!("/api/grants/{FLARE}"),
        200,
        &json!({"share": FLARE, "revoked": []}),
    );
    home.sharing(&["revoke", FLARE, "--everyone", "--yes"])
        .assert_ok();
}

#[test]
fn grants_lists_people_teams_and_everyone_with_their_state() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        &format!("/api/grants/{FLARE}"),
        200,
        &json!({"share": FLARE, "items": [
            {"grantee": "jo@example.org", "kind": "person", "role": "reader", "state": "active",
             "granted_by": "ana@example.org", "created": "2026-11-23T10:00:00Z"},
            {"grantee": "team:design", "kind": "team", "role": "reader", "state": "dormant",
             "granted_by": "ana@example.org", "created": "2026-11-23T10:00:00Z"},
            {"grantee": "everyone", "kind": "everyone", "role": "reader", "state": "active",
             "granted_by": "ana@example.org", "created": "2026-11-23T10:00:00Z",
             "audience": "example.org"},
        ]}),
    );
    let ran = home.sharing(&["grants", FLARE]);
    ran.assert_ok();
    assert!(ran.stdout.contains("dormant"), "{}", ran.all());
    assert!(ran.stdout.contains("team:design"), "{}", ran.all());
    assert!(ran.stdout.contains("everyone"), "{}", ran.all());
}

#[test]
fn shares_with_me_everyone_asks_for_the_shares_open_to_everyone_too() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/shares",
        200,
        &json!({"items": [], "more": false}),
    );
    home.sharing(&["shares", "--with-me", "--everyone"])
        .assert_ok();
    let target = api.requests_to("/api/shares")[0].target.clone();
    assert!(
        target.starts_with("/api/shares?scope=with-me&limit="),
        "{target}"
    );
    assert!(target.ends_with("&everyone=true"), "{target}");
}

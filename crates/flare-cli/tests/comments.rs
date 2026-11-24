//! `flare comments`, `comment`, `reply`, `comment-edit`, `comment-delete`
//! and `activity --since-last` against a fake share API and a fake Keychain:
//! what each sends, what it prints, and how the cursor is kept. Each refusal
//! has a twin that shows the admitted case.
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
use flare_contract::Cursor;
use run::{Home, write};
use serde_json::{Value, json};

const COMMENT: &str = "11111111111111111111111111111111";
const REPLY: &str = "22222222222222222222222222222222";

/// A home whose person is logged in to `api`.
fn logged_in(api: &FakeApi) -> Home {
    let home = Home::new();
    api.issue();
    home.sharing(&["login", &api.origin]).assert_ok();
    home
}

fn comment(id: &str, body: &str) -> Value {
    json!({
        "kind": "comment", "id": id, "flare": FLARE, "author": "jo@example.org",
        "at": "2026-11-23T11:00:00Z", "deleted": false, "body": body, "replies": [],
    })
}

fn reply_to(id: &str, parent: &str, body: &str) -> Value {
    json!({
        "id": id, "flare": FLARE, "parent": parent, "author": "ana@example.org",
        "at": "2026-11-23T12:00:00Z", "deleted": false, "body": body,
    })
}

fn cursor(position: u64) -> String {
    Cursor::new(position).to_string()
}

#[test]
fn comments_prints_each_comment_then_its_replies_and_asks_for_the_flares_thread() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let mut first = comment(COMMENT, "Is this final?");
    first["replies"] = json!([reply_to(REPLY, COMMENT, "Yes, ship it")]);
    api.route(
        "GET",
        "/api/comments",
        200,
        &json!({"flare": FLARE, "comments": [first]}),
    );
    let ran = home.sharing(&["comments", FLARE]);
    ran.assert_ok();
    let first_at = ran.stdout.find("Is this final?").unwrap();
    let reply_at = ran.stdout.find("Yes, ship it").unwrap();
    assert!(first_at < reply_at, "{}", ran.all());
    assert_eq!(
        api.requests_to("/api/comments")[0].target,
        format!("/api/comments?flare={FLARE}")
    );
}

#[test]
fn comment_sends_the_text_and_a_key_and_repeats_a_key_to_get_the_first_comment() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "POST",
        "/api/comments",
        201,
        &comment(COMMENT, "Looks good"),
    );
    api.route(
        "POST",
        "/api/comments",
        200,
        &comment(COMMENT, "Looks good"),
    );
    let first = home.sharing(&[
        "comment",
        FLARE,
        "--body",
        "Looks good",
        "--key",
        "run1",
        "--json",
    ]);
    first.assert_ok();
    let second = home.sharing(&[
        "comment",
        FLARE,
        "--body",
        "Looks good",
        "--key",
        "run1",
        "--json",
    ]);
    second.assert_ok();
    assert_eq!(first.stdout, second.stdout);
    let sent = api.requests_to("/api/comments");
    assert_eq!(
        sent[0].json(),
        json!({"flare": FLARE, "body": "Looks good", "key": "run1"})
    );
    assert_eq!(sent[1].json(), sent[0].json());
}

#[test]
fn comment_without_a_key_draws_a_new_valid_key_each_run() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route("POST", "/api/comments", 201, &comment(COMMENT, "a"));
    api.route("POST", "/api/comments", 201, &comment(COMMENT, "a"));
    home.sharing(&["comment", FLARE, "--body", "a"]).assert_ok();
    home.sharing(&["comment", FLARE, "--body", "a"]).assert_ok();
    let keys: Vec<String> = api
        .requests_to("/api/comments")
        .iter()
        .map(|request| request.json()["key"].as_str().unwrap().to_owned())
        .collect();
    assert_ne!(keys[0], keys[1]);
    for key in &keys {
        assert!((1..=64).contains(&key.len()));
        assert!(
            key.bytes().all(|byte| byte.is_ascii_alphanumeric()),
            "{key}"
        );
    }
}

#[test]
fn comment_reads_its_text_from_a_file_and_refuses_both_sources_or_neither() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("note.txt");
    write(&file, "From a file\nwith two lines\n");
    api.route("POST", "/api/comments", 201, &comment(COMMENT, "x"));
    home.sharing(&["comment", FLARE, "--body-file", file.to_str().unwrap()])
        .assert_ok();
    assert_eq!(
        api.requests_to("/api/comments")[0].json()["body"],
        "From a file\nwith two lines\n"
    );
    for args in [
        vec!["comment", FLARE],
        vec![
            "comment",
            FLARE,
            "--body",
            "x",
            "--body-file",
            file.to_str().unwrap(),
        ],
    ] {
        let ran = home.sharing(&args);
        assert_ne!(ran.code, Some(0), "{}", ran.all());
    }
    assert_eq!(api.requests_to("/api/comments").len(), 1);
    let missing = home.path().join("absent.txt");
    home.sharing(&["comment", FLARE, "--body-file", missing.to_str().unwrap()])
        .assert_failed("flare::files::unreadable");
}

#[test]
fn reply_names_its_comment_and_flare_and_a_refused_reply_keeps_the_servers_code() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route("POST", "/api/comments", 201, &json!({"kind": "reply", "id": REPLY, "flare": FLARE, "parent": COMMENT, "author": "ana@example.org", "at": "2026-11-23T12:00:00Z", "deleted": false, "body": "ok"}));
    home.sharing(&[
        "reply", COMMENT, "--flare", FLARE, "--body", "ok", "--key", "r1",
    ])
    .assert_ok();
    assert_eq!(
        api.requests_to("/api/comments")[0].json(),
        json!({"flare": FLARE, "parent": COMMENT, "body": "ok", "key": "r1"})
    );
    api.route(
        "POST",
        "/api/comments",
        400,
        &problem("flare::comments::reply_depth", 400, &api.origin),
    );
    home.sharing(&["reply", REPLY, "--flare", FLARE, "--body", "deeper"])
        .assert_failed("flare::comments::reply_depth");
}

#[test]
fn comment_edit_sends_a_patch_and_comment_delete_a_delete_and_only_with_yes() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let path = format!("/api/comments/{COMMENT}");
    api.route("PATCH", &path, 200, &comment(COMMENT, "better"));
    home.sharing(&["comment-edit", COMMENT, "--body", "better"])
        .assert_ok();
    let patched = &api.requests_to(&path)[0];
    assert_eq!(
        (patched.method.as_str(), patched.json()),
        ("PATCH", json!({"body": "better"}))
    );
    let declined = home.sharing(&["comment-delete", COMMENT]);
    assert_eq!(declined.code, Some(1), "{}", declined.all());
    assert_eq!(api.requests_to(&path).len(), 1);
    api.route("DELETE", &path, 200, &json!({"kind": "comment", "id": COMMENT, "flare": FLARE, "author": "ana@example.org", "at": "2026-11-23T11:00:00Z", "deleted": true, "replies": []}));
    home.sharing(&["comment-delete", COMMENT, "--yes"])
        .assert_ok();
    assert_eq!(api.requests_to(&path)[1].method, "DELETE");
}

#[test]
fn a_refused_edit_keeps_the_servers_code_and_a_malformed_comment_id_is_not_found() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "PATCH",
        &format!("/api/comments/{COMMENT}"),
        403,
        &problem("flare::comments::not_author", 403, &api.origin),
    );
    home.sharing(&["comment-edit", COMMENT, "--body", "mine now"])
        .assert_failed("flare::comments::not_author");
    home.sharing(&["comment-edit", "not-an-id", "--body", "x"])
        .assert_failed("flare::comments::not_found");
    assert_eq!(
        api.requests()
            .iter()
            .filter(|r| r.method == "PATCH")
            .count(),
        1
    );
}

fn activity_page(items: &[Value], at: u64, more: bool) -> Value {
    json!({"items": items, "cursor": cursor(at), "more": more})
}

fn comment_item() -> Value {
    json!({"kind": "comment", "flare": FLARE, "comment": COMMENT, "by": "jo@example.org",
           "body": "A new comment", "at": "2026-11-23T11:00:00Z"})
}

#[test]
fn activity_lists_comment_items_beside_responses() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let deleted = json!({"kind": "deleted", "flare": FLARE, "comment": REPLY, "parent": COMMENT,
        "by": "jo@example.org", "at": "2026-11-23T13:00:00Z"});
    api.route(
        "GET",
        "/api/activity",
        200,
        &activity_page(&[comment_item(), deleted], 5, false),
    );
    let ran = home.sharing(&["activity", FLARE]);
    ran.assert_ok();
    for expected in ["comment", "A new comment", "deleted"] {
        assert!(ran.stdout.contains(expected), "{expected}: {}", ran.all());
    }
}

#[test]
fn since_last_reads_from_the_larger_cursor_and_saves_it_here_before_printing_and_on_the_server() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/activity",
        200,
        &activity_page(&[comment_item()], 8, false),
    );
    api.route(
        "PUT",
        "/api/cursors/inbox",
        200,
        &json!({"cursor": cursor(8)}),
    );
    let first = home.sharing(&["activity", FLARE, "--since-last", "inbox"]);
    first.assert_ok();
    assert!(first.stdout.contains("A new comment"), "{}", first.all());
    assert_eq!(
        api.requests_to("/api/activity")[0].target,
        format!("/api/activity?flare={FLARE}&since_last=inbox&limit=50")
    );
    let saved = &api.requests_to("/api/cursors/inbox")[0];
    assert_eq!(
        (saved.method.as_str(), saved.json()),
        ("PUT", json!({"cursor": cursor(8)}))
    );
    // The next call starts from the saved position, and a smaller --since
    // does not move it back.
    api.route("GET", "/api/activity", 200, &activity_page(&[], 8, false));
    api.route(
        "PUT",
        "/api/cursors/inbox",
        200,
        &json!({"cursor": cursor(8)}),
    );
    home.sharing(&[
        "activity",
        FLARE,
        "--since-last",
        "inbox",
        "--since",
        &cursor(3),
    ])
    .assert_ok();
    assert_eq!(
        api.requests_to("/api/activity")[1].target,
        format!(
            "/api/activity?flare={FLARE}&since={}&since_last=inbox&limit=50",
            cursor(8)
        )
    );
    // A call without --since-last saves nothing.
    api.route("GET", "/api/activity", 200, &activity_page(&[], 9, false));
    home.sharing(&["activity", FLARE]).assert_ok();
    assert_eq!(api.requests_to("/api/cursors/inbox").len(), 2);
}

#[test]
fn a_cursor_that_the_server_refuses_to_save_is_noted_and_the_local_one_still_moves() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/activity",
        200,
        &activity_page(&[comment_item()], 8, false),
    );
    api.route(
        "PUT",
        "/api/cursors/inbox",
        503,
        &problem("flare::comments::unavailable", 503, &api.origin),
    );
    let ran = home.sharing(&["activity", FLARE, "--since-last", "inbox"]);
    ran.assert_ok();
    assert!(ran.stdout.contains("A new comment"), "{}", ran.all());
    assert!(
        ran.stderr.contains("did not save the cursor inbox"),
        "{}",
        ran.all()
    );
    api.route("GET", "/api/activity", 200, &activity_page(&[], 8, false));
    api.route(
        "PUT",
        "/api/cursors/inbox",
        200,
        &json!({"cursor": cursor(8)}),
    );
    home.sharing(&["activity", FLARE, "--since-last", "inbox"])
        .assert_ok();
    assert_eq!(
        api.requests_to("/api/activity")[1].target,
        format!(
            "/api/activity?flare={FLARE}&since={}&since_last=inbox&limit=50",
            cursor(8)
        )
    );
}

#[test]
fn an_activity_call_that_fails_saves_no_cursor_and_a_bad_name_is_refused_before_any_request() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/activity",
        404,
        &problem("flare::shares::not_found", 404, &api.origin),
    );
    home.sharing(&["activity", FLARE, "--since-last", "inbox"])
        .assert_failed("flare::shares::not_found");
    assert!(api.requests_to("/api/cursors/inbox").is_empty());
    let before = api.requests().len();
    home.sharing(&["activity", FLARE, "--since-last", "has space"])
        .assert_failed("flare::requests::bad_query");
    assert_eq!(api.requests().len(), before);
}

#[test]
fn an_unreadable_cursors_file_fails_the_command_before_it_prints_and_a_control_run_without_it_prints()
 {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/activity",
        200,
        &activity_page(&[comment_item()], 8, false),
    );
    api.route(
        "PUT",
        "/api/cursors/inbox",
        200,
        &json!({"cursor": cursor(8)}),
    );
    let directory = home.state().join("flare");
    write(&directory.join("cursors.json"), "not json");
    let broken = home.sharing(&["activity", FLARE, "--since-last", "inbox"]);
    broken.assert_failed("flare::state::unusable");
    assert!(!broken.stdout.contains("A new comment"), "{}", broken.all());
    assert!(api.requests_to("/api/cursors/inbox").is_empty());
    std::fs::remove_file(directory.join("cursors.json")).unwrap();
    let healthy = home.sharing(&["activity", FLARE, "--since-last", "inbox"]);
    healthy.assert_ok();
    assert!(
        healthy.stdout.contains("A new comment"),
        "{}",
        healthy.all()
    );
}

#[test]
fn a_since_last_call_names_the_next_call_with_the_saved_name_and_a_plain_call_with_the_cursor() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/activity",
        200,
        &activity_page(&[comment_item()], 8, false),
    );
    api.route(
        "PUT",
        "/api/cursors/inbox",
        200,
        &json!({"cursor": cursor(8)}),
    );
    let ran = home.sharing(&["activity", FLARE, "--since-last", "inbox", "--json"]);
    ran.assert_ok();
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        printed["next"],
        json!([{
            "text": "Read the activity that arrives after this.",
            "commands": [{"call": format!("flare activity {FLARE} --since-last inbox")}],
        }]),
        "{}",
        ran.all()
    );
    assert_eq!(
        printed["summary"],
        json!({"cursor": cursor(8), "saved_as": "inbox", "more": false}),
        "{}",
        ran.all()
    );
    // The twin without --since-last names the cursor.
    api.route(
        "GET",
        "/api/activity",
        200,
        &activity_page(&[comment_item()], 9, false),
    );
    let ran = home.sharing(&["activity", FLARE, "--json"]);
    ran.assert_ok();
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        printed["next"][0]["commands"][0]["call"],
        format!("flare activity {FLARE} --since {}", cursor(9)),
        "{}",
        ran.all()
    );
}

/// The server's answer to a request that failed while it ran.
fn fault() -> Value {
    json!({
        "type": "https://flare.dev/problems/server::fault",
        "title": "The server failed while answering. The failure is logged.",
        "status": 500,
        "code": "server::fault",
    })
}

/// The key that the `index`th request to `/api/comments` sent.
fn sent_key(api: &FakeApi, index: usize) -> String {
    api.requests_to("/api/comments")[index].json()["key"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn a_comment_the_server_failed_on_recovers_by_repeating_the_command_with_the_key_it_drew() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route("POST", "/api/comments", 500, &fault());
    let ran = home.sharing(&["comment", FLARE, "--body", "Looks good", "--json"]);
    ran.assert_failed("flare::network::server_error");
    let key = sent_key(&api, 0);
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        printed["recovery"],
        json!({"kind": "run", "call": format!("flare comment {FLARE} --body <TEXT> --key {key}")}),
        "{}",
        ran.all()
    );
    // The twin with --key already repeats the key, so it runs again as it is.
    api.route("POST", "/api/comments", 500, &fault());
    let ran = home.sharing(&[
        "comment",
        FLARE,
        "--body",
        "Looks good",
        "--key",
        "run1",
        "--json",
    ]);
    ran.assert_failed("flare::network::server_error");
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        printed["recovery"],
        json!({"kind": "retry"}),
        "{}",
        ran.all()
    );
}

#[test]
fn a_reply_the_comment_store_did_not_answer_recovers_by_repeating_the_command_with_the_key_it_drew()
{
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("reply.txt");
    write(&file, "Agreed\n");
    api.route(
        "POST",
        "/api/comments",
        503,
        &problem("flare::comments::unavailable", 503, &api.origin),
    );
    let ran = home.sharing(&[
        "reply",
        COMMENT,
        "--flare",
        FLARE,
        "--body-file",
        file.to_str().unwrap(),
        "--json",
    ]);
    ran.assert_failed("flare::comments::unavailable");
    let key = sent_key(&api, 0);
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        printed["recovery"],
        json!({"kind": "run", "call": format!(
            "flare reply {COMMENT} --flare {FLARE} --body-file {} --key {key}",
            file.to_str().unwrap()
        )}),
        "{}",
        ran.all()
    );
}

#[test]
fn a_comment_the_server_refused_before_it_ran_keeps_its_own_recovery() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "POST",
        "/api/comments",
        503,
        &json!({
            "type": "https://flare.dev/problems/server::busy",
            "title": "The server is busy. Try again.",
            "status": 503,
            "code": "server::busy",
        }),
    );
    let ran = home.sharing(&["comment", FLARE, "--body", "Looks good", "--json"]);
    ran.assert_failed("flare::network::server_error");
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        printed["recovery"],
        json!({"kind": "retry"}),
        "{}",
        ran.all()
    );
}

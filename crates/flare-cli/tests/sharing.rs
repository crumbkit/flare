//! `flare login`, `logout`, `share`, `grant`, `shares` and `activity`
//! against a fake share API and a fake Keychain: what each sends, what it
//! prints, and how each refusal keeps the server's code and recovery. Each
//! refusal has a twin that shows the admitted case.
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

use api::{CODE, FLARE, FakeApi, Fakes as _, TOKEN, TOKEN_ID, problem};
use run::{Home, write};
use serde_json::{Value, json};

/// A home whose person is logged in to `api`.
fn logged_in(api: &FakeApi) -> Home {
    let home = Home::new();
    api.issue();
    home.sharing(&["login", &api.origin]).assert_ok();
    home
}

/// A share summary as the API sends it.
fn summary(id: &str, title: &str) -> Value {
    json!({
        "id": id,
        "title": title,
        "format": "markdown",
        "owner": "ana@example.org",
        "role": "owner",
        "version": 1,
        "created": "2026-11-23T10:00:00Z",
    })
}

/// One response in an activity page.
fn response(id: &str, title: &str) -> Value {
    json!({
        "kind": "response",
        "flare": FLARE,
        "share": id,
        "title": title,
        "format": "markdown",
        "by": "jo@example.org",
        "version": 1,
        "at": "2026-11-23T11:00:00Z",
        "also": [],
    })
}

#[test]
fn login_saves_the_token_in_the_keychain_through_standard_input_and_never_on_a_command_line() {
    let api = FakeApi::start();
    let home = Home::new();
    api.issue();
    let ran = home.sharing(&["login", &api.origin, "--json"]);
    ran.assert_ok();
    assert!(ran.stdout.contains("ana@example.org"), "{}", ran.all());
    assert!(!ran.all().contains(TOKEN));

    let opened = std::fs::read_to_string(home.path().join("opened.txt")).unwrap();
    assert!(
        opened.starts_with(&format!("{}/cli/login?port=", api.origin)),
        "{opened}"
    );
    let exchanged = api.requests_to("/api/tokens/exchange");
    assert_eq!(exchanged.len(), 1);
    assert_eq!(exchanged[0].json()["code"], CODE);
    assert!(
        exchanged[0].json()["label"]
            .as_str()
            .unwrap()
            .starts_with("flare-cli-")
    );
    assert_eq!(
        home.keychain_item(),
        Some(format!("{} {TOKEN}", api.origin))
    );
    assert!(home.keychain_calls().contains("-i"));
    assert!(!home.keychain_calls().contains(TOKEN));
}

#[test]
fn login_fails_with_login_failed_when_the_callback_carries_another_state() {
    let api = FakeApi::start();
    let home = Home::new();
    home.login_opener(Some("0000000000000000aaaaaaaaaaaaaaaa"));
    api.issue();
    let ran = home.sharing(&["login", &api.origin]);
    ran.assert_failed("flare::login::failed");
    assert!(api.requests_to("/api/tokens/exchange").is_empty());
    assert_eq!(home.keychain_item(), None);
}

#[test]
fn login_refuses_a_server_that_is_not_an_https_or_loopback_origin() {
    let home = Home::new();
    let ran = home.sharing(&["login", "http://flare.example.org"]);
    ran.assert_failed("flare::login::invalid_server");
}

#[test]
fn a_refused_login_code_keeps_the_servers_code_and_names_flare_login_with_the_server() {
    let api = FakeApi::start();
    let home = Home::new();
    api.route(
        "POST",
        "/api/tokens/exchange",
        403,
        &problem("flare::tokens::code_refused", 403, &api.origin),
    );
    let ran = home.sharing(&["login", &api.origin]);
    ran.assert_failed("flare::tokens::code_refused");
    assert!(
        ran.all().contains(&format!("flare login {}", api.origin)),
        "{}",
        ran.all()
    );
    assert_eq!(home.keychain_item(), None);
}

#[test]
fn a_second_login_keeps_the_first_logins_label_so_the_server_revokes_the_old_token() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.issue();
    home.sharing(&["login", &api.origin]).assert_ok();
    let labels: Vec<Value> = api
        .requests_to("/api/tokens/exchange")
        .iter()
        .map(|request| request.json()["label"].clone())
        .collect();
    assert_eq!(labels.len(), 2);
    assert_eq!(labels[0], labels[1]);
}

#[test]
fn share_sends_the_file_with_each_recipient_and_flare_and_the_saved_token() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    api.route(
        "POST",
        "/api/shares",
        201,
        &json!({
            "share": summary("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "review"),
            "granted": ["ana@example.org", "jo@example.org", "mei@example.org"],
            "responds_to": [FLARE],
        }),
    );
    let ran = home.sharing(&[
        "share",
        file.to_str().unwrap(),
        "--with",
        "ana@example.org,jo@example.org",
        "--with",
        "mei@example.org",
        "--re",
        FLARE,
        "--json",
    ]);
    ran.assert_ok();
    assert!(
        ran.stdout.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "{}",
        ran.all()
    );
    let sent = &api.requests_to("/api/shares")[0];
    assert_eq!(
        sent.headers.get("authorization").map(String::as_str),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert_eq!(
        sent.json(),
        json!({
            "title": "review",
            "format": "markdown",
            "content": "# Review\n",
            "with": ["ana@example.org", "jo@example.org", "mei@example.org"],
            "re": [FLARE],
        })
    );
}

#[test]
fn share_without_a_login_fails_with_login_needed_and_names_flare_login() {
    let home = Home::new();
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    let ran = home.sharing(&["share", file.to_str().unwrap()]);
    ran.assert_failed("flare::login::needed");
    assert!(ran.all().contains("flare login"), "{}", ran.all());
}

#[test]
fn a_refusal_from_the_server_keeps_its_code_and_its_hint() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    let mut refusal = problem("flare::shares::not_a_creator", 403, &api.origin);
    refusal["recovery"] = json!({"kind": "hint", "text": "ask the operator to add you"});
    api.route("POST", "/api/shares", 403, &refusal);
    let ran = home.sharing(&["share", file.to_str().unwrap()]);
    ran.assert_failed("flare::shares::not_a_creator");
    assert!(ran.all().contains("operator"), "{}", ran.all());
}

#[test]
fn an_expired_token_fails_with_sign_in_needed_and_names_flare_login_with_the_server() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/shares",
        401,
        &problem("flare::tokens::sign_in_needed", 401, &api.origin),
    );
    let ran = home.sharing(&["shares"]);
    ran.assert_failed("flare::tokens::sign_in_needed");
    assert!(
        ran.all().contains(&format!("flare login {}", api.origin)),
        "{}",
        ran.all()
    );
}

#[test]
fn share_refuses_a_file_that_is_neither_markdown_nor_html() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("review.txt");
    write(&file, "text\n");
    home.sharing(&["share", file.to_str().unwrap()])
        .assert_failed("flare::files::unknown_format");
    assert!(api.requests_to("/api/shares").is_empty());
}

#[test]
fn grant_sends_each_recipient_and_flare_to_the_shares_grant_route() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "POST",
        &format!("/api/grants/{FLARE}"),
        200,
        &json!({
            "share": summary(FLARE, "Plans"),
            "granted": ["mei@example.org"],
            "responds_to": [],
        }),
    );
    home.sharing(&["grant", FLARE, "--with", "mei@example.org"])
        .assert_ok();
    let sent = &api.requests_to(&format!("/api/grants/{FLARE}"))[0];
    assert_eq!(sent.json(), json!({"with": ["mei@example.org"], "re": []}));
}

#[test]
fn shares_with_me_asks_for_the_shares_granted_to_the_caller_up_to_the_limit() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/shares",
        200,
        &json!({"items": [summary(FLARE, "Plans")], "more": false}),
    );
    let ran = home.sharing(&["shares", "--with-me", "--limit", "20", "--json"]);
    ran.assert_ok();
    assert!(ran.stdout.contains("Plans"), "{}", ran.all());
    assert_eq!(
        api.requests_to("/api/shares")[0].target,
        "/api/shares?scope=with-me&limit=20"
    );
}

#[test]
fn activity_with_all_follows_the_cursor_until_no_more_and_prints_the_last_cursor() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/activity",
        200,
        &json!({
            "items": [response("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "First")],
            "cursor": "fc1.AAAAAAAAAAE",
            "more": true,
        }),
    );
    api.route(
        "GET",
        "/api/activity",
        200,
        &json!({
            "items": [response("cccccccccccccccccccccccccccccccc", "Second")],
            "cursor": "fc1.AAAAAAAAAAI",
            "more": false,
        }),
    );
    let ran = home.sharing(&[
        "activity",
        FLARE,
        "--since",
        "fc1.AAAAAAAAAAA",
        "--all",
        "--json",
    ]);
    ran.assert_ok();
    for expected in ["First", "Second", "fc1.AAAAAAAAAAI"] {
        assert!(ran.stdout.contains(expected), "{expected}: {}", ran.all());
    }
    let targets: Vec<String> = api
        .requests_to("/api/activity")
        .into_iter()
        .map(|request| request.target)
        .collect();
    assert_eq!(
        targets,
        [
            format!("/api/activity?flare={FLARE}&since=fc1.AAAAAAAAAAA&limit=100"),
            format!("/api/activity?flare={FLARE}&since=fc1.AAAAAAAAAAE&limit=100"),
        ]
    );
}

#[test]
fn activity_without_all_reads_one_page_and_prints_its_cursor() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/activity",
        200,
        &json!({"items": [], "cursor": "fc1.AAAAAAAAAC8", "more": false}),
    );
    let ran = home.sharing(&["activity", FLARE, "--json"]);
    ran.assert_ok();
    assert!(ran.stdout.contains("fc1.AAAAAAAAAC8"), "{}", ran.all());
    assert_eq!(
        api.requests_to("/api/activity")[0].target,
        format!("/api/activity?flare={FLARE}&limit=50")
    );
}

#[test]
fn logout_revokes_the_token_and_forgets_it() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "DELETE",
        &format!("/api/tokens/{TOKEN_ID}"),
        204,
        &Value::Null,
    );
    home.sharing(&["logout", "--yes"]).assert_ok();
    let revoked = api.requests_to(&format!("/api/tokens/{TOKEN_ID}"));
    assert_eq!(revoked.len(), 1);
    assert_eq!(revoked[0].method, "DELETE");
    assert_eq!(home.keychain_item(), None);
    home.sharing(&["shares"])
        .assert_failed("flare::login::needed");
}

#[test]
fn a_server_that_does_not_answer_fails_with_network_unreachable() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let gone = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = gone.local_addr().unwrap().port();
    drop(gone);
    let saved = home.state().join("flare").join("login.json");
    let text = std::fs::read_to_string(&saved).unwrap();
    let moved = format!("http://127.0.0.1:{port}");
    write(&saved, &text.replace(&api.origin, &moved));
    write(
        &home.path().join("keychain-item"),
        &format!("{moved} {TOKEN}\n"),
    );
    home.sharing(&["shares"])
        .assert_failed("flare::network::unreachable");
}

#[test]
fn logout_without_yes_and_without_a_terminal_revokes_nothing() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let ran = home.sharing(&["logout"]);
    assert_eq!(ran.code, Some(1), "{}", ran.all());
    assert!(
        api.requests_to(&format!("/api/tokens/{TOKEN_ID}"))
            .is_empty()
    );
    assert!(home.keychain_item().is_some());
}

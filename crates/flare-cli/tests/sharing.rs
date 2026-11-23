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
fn a_login_whose_exchange_answer_is_lost_gives_its_label_to_the_next_login() {
    let api = FakeApi::start();
    let home = Home::new();
    api.route("POST", "/api/tokens/exchange", 502, &json!("lost"));
    home.sharing(&["login", &api.origin])
        .assert_failed("flare::network::bad_answer");
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
fn a_login_whose_keychain_refuses_the_token_gives_its_label_to_the_next_login() {
    let api = FakeApi::start();
    let home = Home::new();
    let refusing = home.script("refusing-security", "exit 1\n");
    api.issue();
    #[expect(clippy::disallowed_methods, reason = "the tests run the built binary")]
    let refused = home
        .command(&["login", &api.origin])
        .env("FLARE_KEYCHAIN", refusing)
        .env("FLARE_OPENER", home.login_opener(None))
        .output()
        .unwrap();
    run::Ran::from(refused).assert_failed("flare::keychain::unavailable");
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
fn share_update_posts_the_file_as_the_next_version_and_prints_the_share_with_its_version() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("review.md");
    write(&file, "# Review 2\n");
    let mut updated = summary(FLARE, "review");
    updated["version"] = json!(2);
    api.route(
        "POST",
        &format!("/api/shares/{FLARE}/versions"),
        201,
        &updated,
    );
    let ran = home.sharing(&["share", file.to_str().unwrap(), "--update", FLARE, "--json"]);
    ran.assert_ok();
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(printed["data"]["id"], FLARE, "{}", ran.all());
    assert_eq!(printed["data"]["title"], "review");
    assert_eq!(printed["data"]["version"], 2);
    assert!(printed["data"]["granted"].is_null(), "{}", ran.all());
    assert!(printed["data"]["responds_to"].is_null(), "{}", ran.all());
    let sent = &api.requests_to(&format!("/api/shares/{FLARE}/versions"))[0];
    assert_eq!(
        sent.headers.get("authorization").map(String::as_str),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert_eq!(
        sent.json(),
        json!({"format": "markdown", "content": "# Review 2\n"})
    );
    assert!(api.requests_to("/api/shares").is_empty());
}

#[test]
fn share_update_refuses_title_with_and_re_before_any_request_and_a_plain_update_is_admitted() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    let file = file.to_str().unwrap();
    for extra in [
        ["--title", "New"],
        ["--with", "jo@example.org"],
        ["--re", FLARE],
    ] {
        let ran = home.sharing(&["share", file, "--update", FLARE, extra[0], extra[1]]);
        assert_ne!(ran.code, Some(0), "{}", ran.all());
        assert!(ran.all().contains(extra[0]), "{}", ran.all());
    }
    assert!(
        api.requests_to(&format!("/api/shares/{FLARE}/versions"))
            .is_empty()
    );
    let mut updated = summary(FLARE, "review");
    updated["version"] = json!(2);
    api.route(
        "POST",
        &format!("/api/shares/{FLARE}/versions"),
        201,
        &updated,
    );
    home.sharing(&["share", file, "--update", FLARE])
        .assert_ok();
}

#[test]
fn share_update_keeps_the_servers_refusal_code_for_a_share_the_caller_cannot_publish() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    api.route(
        "POST",
        &format!("/api/shares/{FLARE}/versions"),
        404,
        &problem("flare::shares::not_found", 404, &api.origin),
    );
    home.sharing(&["share", file.to_str().unwrap(), "--update", FLARE])
        .assert_failed("flare::shares::not_found");
}

#[test]
fn share_update_with_a_malformed_id_or_an_unknown_file_ending_sends_nothing() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    home.sharing(&["share", file.to_str().unwrap(), "--update", "not an id"])
        .assert_failed("flare::shares::not_found");
    let text = home.path().join("review.txt");
    write(&text, "text\n");
    home.sharing(&["share", text.to_str().unwrap(), "--update", FLARE])
        .assert_failed("flare::files::unknown_format");
    assert!(
        api.requests_to(&format!("/api/shares/{FLARE}/versions"))
            .is_empty()
    );
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

/// A share opened at `version` of `latest`, as the API sends it.
fn opened(version: u32, latest: u32, content: &str) -> Value {
    let mut share = summary(FLARE, "Plans");
    share["role"] = json!("reader");
    share["version"] = json!(latest);
    json!({"share": share, "version": version, "content": content})
}

#[test]
fn show_asks_for_the_version_and_prints_the_title_owner_role_version_and_content() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let path = format!("/api/shares/{FLARE}");
    api.route("GET", &path, 200, &opened(1, 2, "# Plans\n\nFirst cut.\n"));
    let ran = home.sharing(&["show", FLARE, "--version", "1", "--json"]);
    ran.assert_ok();
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(printed["schema"], "flare/show/v1", "{}", ran.all());
    let data = &printed["data"];
    assert_eq!(data["id"], FLARE);
    assert_eq!(data["title"], "Plans");
    assert_eq!(data["owner"], "ana@example.org");
    assert_eq!(data["role"], "reader");
    assert_eq!(data["version"], 1);
    assert_eq!(data["latest"], 2);
    assert_eq!(data["content"], "# Plans\n\nFirst cut.\n");
    let sent = &api.requests_to(&path)[0];
    assert_eq!(sent.method, "GET");
    assert_eq!(sent.target, format!("{path}?version=1"));
    assert_eq!(
        sent.headers.get("authorization").map(String::as_str),
        Some(format!("Bearer {TOKEN}").as_str())
    );

    api.route("GET", &path, 200, &opened(2, 2, "# Plans\n\nSecond cut.\n"));
    let ran = home.sharing(&["show", FLARE]);
    ran.assert_ok();
    for expected in ["Plans", "ana@example.org", "reader", "Second cut."] {
        assert!(ran.stdout.contains(expected), "{expected}: {}", ran.all());
    }
    assert_eq!(api.requests_to(&path)[1].target, path);
}

#[test]
fn show_without_a_login_fails_with_login_needed_and_a_version_of_zero_sends_nothing() {
    let home = Home::new();
    home.sharing(&["show", FLARE])
        .assert_failed("flare::login::needed");
    let api = FakeApi::start();
    let home = logged_in(&api);
    for version in ["0", "abc"] {
        let ran = home.sharing(&["show", FLARE, "--version", version]);
        assert_ne!(ran.code, Some(0), "{}", ran.all());
    }
    assert!(api.requests_to(&format!("/api/shares/{FLARE}")).is_empty());
}

#[test]
fn show_keeps_the_servers_not_found_for_a_share_the_caller_cannot_open() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        &format!("/api/shares/{FLARE}"),
        404,
        &problem("flare::shares::not_found", 404, &api.origin),
    );
    home.sharing(&["show", FLARE, "--version", "9"])
        .assert_failed("flare::shares::not_found");
}

#[test]
fn a_dry_run_with_no_login_checks_the_file_prints_the_checks_and_sends_nothing() {
    let api = FakeApi::start();
    let home = Home::new();
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    let ran = home.sharing(&[
        "share",
        file.to_str().unwrap(),
        "--with",
        "jo@example.org",
        "--dry-run",
        "--json",
    ]);
    ran.assert_ok();
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(printed["dry_run"], true, "{}", ran.all());
    let data = &printed["data"];
    assert_eq!(data["title"], "review");
    assert!(data["id"].is_null(), "{}", ran.all());
    assert!(data["version"].is_null(), "{}", ran.all());
    let checks = data["checks"].to_string();
    for expected in ["Markdown", "title", "rendered", "--with"] {
        assert!(checks.contains(expected), "{expected}: {checks}");
    }
    assert!(api.requests().is_empty());
    assert_eq!(home.keychain_calls(), "");
}

#[test]
fn a_dry_run_with_a_saved_login_reads_neither_the_keychain_nor_the_server() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let calls = home.keychain_calls();
    let requests = api.requests().len();
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    home.sharing(&["share", file.to_str().unwrap(), "--dry-run"])
        .assert_ok();
    home.sharing(&[
        "share",
        file.to_str().unwrap(),
        "--update",
        FLARE,
        "--dry-run",
    ])
    .assert_ok();
    assert_eq!(home.keychain_calls(), calls);
    assert_eq!(api.requests().len(), requests);
}

#[test]
fn a_dry_run_of_a_file_the_server_would_refuse_prints_the_refusal_and_writes_no_html() {
    let home = Home::new();
    let out = home.path().join("preview.html");
    let empty = home.path().join("empty.md");
    write(&empty, "");
    let large = home.path().join("large.md");
    write(&large, &"x".repeat(262_145));
    let html = home.path().join("page.html");
    write(&html, "<p>x</p>\n");
    let text = home.path().join("notes.txt");
    write(&text, "x\n");
    let good = home.path().join("good.md");
    write(&good, "# Good\n");
    let long_title = "t".repeat(201);
    let cases: [(&std::path::Path, &[&str], &str); 5] = [
        (&empty, &[], "flare::shares::invalid"),
        (&large, &[], "flare::shares::too_large"),
        (&html, &[], "flare::shares::kind_not_served"),
        (&text, &[], "flare::files::unknown_format"),
        (&good, &["--title", &long_title], "flare::shares::invalid"),
    ];
    for (file, extra, code) in cases {
        let mut args = vec!["share", file.to_str().unwrap(), "--dry-run"];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["--out", out.to_str().unwrap()]);
        home.sharing(&args).assert_failed(code);
        assert!(!out.exists(), "{code}");
    }
    home.sharing(&["share", good.to_str().unwrap(), "--dry-run"])
        .assert_ok();
}

#[test]
fn a_dry_run_with_out_writes_the_html_that_flare_render_makes_for_the_same_text() {
    let home = Home::new();
    let text = "# Review\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n<script>x</script>\n";
    let file = home.path().join("review.md");
    write(&file, text);
    let out = home.path().join("preview.html");
    let ran = home.sharing(&[
        "share",
        file.to_str().unwrap(),
        "--dry-run",
        "--out",
        out.to_str().unwrap(),
        "--json",
    ]);
    ran.assert_ok();
    let page = flare_core::Page::new(
        flare_core::PageName::try_from("share").unwrap(),
        flare_core::PageContent::new(flare_core::ContentFormat::Markdown, text).unwrap(),
        flare_core::Slots::new(Vec::new()).unwrap(),
    );
    let expected = flare_render::render_page(&page, &[]).unwrap();
    assert_eq!(std::fs::read_to_string(&out).unwrap(), expected.as_str());
    assert!(ran.stdout.contains("preview.html"), "{}", ran.all());
}

#[test]
fn out_without_dry_run_is_refused_before_any_request() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    let out = home.path().join("preview.html");
    let ran = home.sharing(&[
        "share",
        file.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_ne!(ran.code, Some(0), "{}", ran.all());
    assert!(api.requests_to("/api/shares").is_empty());
    assert!(!out.exists());
}

#[test]
fn a_dry_run_whose_out_file_cannot_be_written_fails_with_files_unwritable() {
    let home = Home::new();
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    let out = home.path().join("missing").join("preview.html");
    home.sharing(&[
        "share",
        file.to_str().unwrap(),
        "--dry-run",
        "--out",
        out.to_str().unwrap(),
    ])
    .assert_failed("flare::files::unwritable");
    assert!(!out.exists());
    let out = home.path().join("preview.html");
    home.sharing(&[
        "share",
        file.to_str().unwrap(),
        "--dry-run",
        "--out",
        out.to_str().unwrap(),
    ])
    .assert_ok();
    assert!(out.exists());
}

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

/// The `recovery` of the `flare/failure/v1` document that a failed
/// `--json` run prints.
fn recovery(ran: &run::Ran) -> Value {
    let failure: Value =
        serde_json::from_str(&ran.stdout).unwrap_or_else(|error| panic!("{error}: {}", ran.all()));
    assert_eq!(failure["schema"], "flare/failure/v1", "{}", ran.all());
    failure["recovery"].clone()
}

#[test]
fn a_login_whose_browser_sign_in_fails_gives_a_terminal_recovery_for_flare_login_with_the_server() {
    let api = FakeApi::start();
    let home = Home::new();
    home.login_opener(Some("0000000000000000aaaaaaaaaaaaaaaa"));
    api.issue();

    let ran = home.sharing(&["login", &api.origin, "--json"]);

    ran.assert_failed("flare::login::failed");
    let recovery = recovery(&ran);
    assert_eq!(recovery["kind"], "terminal", "{}", ran.all());
    assert_eq!(
        recovery["call"],
        format!("flare login {}", api.origin),
        "{}",
        ran.all()
    );
}

#[test]
fn a_command_with_no_saved_login_hands_the_sign_in_to_a_person_as_a_hint() {
    let home = Home::new();

    let ran = home.sharing(&["shares", "--json"]);

    ran.assert_failed("flare::login::needed");
    let recovery = recovery(&ran);
    assert_eq!(recovery["kind"], "hint", "{}", ran.all());
    assert!(
        recovery["text"].as_str().unwrap().contains("flare login"),
        "{}",
        ran.all()
    );
}

#[test]
fn a_saved_login_whose_token_is_gone_gives_a_terminal_recovery_for_flare_login_with_the_server() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    std::fs::remove_file(home.path().join("keychain-item")).unwrap();

    let ran = home.sharing(&["shares", "--json"]);

    ran.assert_failed("flare::login::needed");
    let recovery = recovery(&ran);
    assert_eq!(recovery["kind"], "terminal", "{}", ran.all());
    assert_eq!(
        recovery["call"],
        format!("flare login {}", api.origin),
        "{}",
        ran.all()
    );
}

#[test]
fn each_refusal_that_needs_a_new_sign_in_gives_a_terminal_recovery_for_flare_login_with_the_server()
{
    for (code, status) in [
        ("flare::tokens::sign_in_needed", 401),
        ("flare::tokens::code_refused", 403),
        ("flare::shares::no_email", 403),
    ] {
        let api = FakeApi::start();
        let home = logged_in(&api);
        api.route(
            "GET",
            "/api/shares",
            status,
            &problem(code, status, &api.origin),
        );

        let ran = home.sharing(&["shares", "--json"]);

        ran.assert_failed(code);
        let recovery = recovery(&ran);
        assert_eq!(recovery["kind"], "terminal", "{code}: {}", ran.all());
        assert_eq!(
            recovery["call"],
            format!("flare login {}", api.origin),
            "{code}: {}",
            ran.all()
        );
    }
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
    let ran = home.sharing(&["shares", "--json"]);
    let printed = failure(&ran, "flare::network::unreachable");
    assert_eq!(
        printed["recovery"],
        json!({"kind": "retry"}),
        "{}",
        ran.all()
    );
    // A refused connection sent nothing, so a share can run again.
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    let ran = home.sharing(&["share", file.to_str().unwrap(), "--json"]);
    let printed = failure(&ran, "flare::network::unreachable");
    assert_eq!(
        printed["recovery"],
        json!({"kind": "retry"}),
        "{}",
        ran.all()
    );
}

#[test]
fn a_login_whose_server_does_not_answer_the_exchange_hands_the_sign_in_to_a_person() {
    let home = Home::new();
    let gone = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let origin = format!("http://127.0.0.1:{}", gone.local_addr().unwrap().port());
    drop(gone);
    let ran = home.sharing(&["login", &origin, "--json"]);
    let printed = failure(&ran, "flare::network::unreachable");
    assert_eq!(
        printed["recovery"],
        json!({"kind": "terminal", "call": format!("flare login {origin}")}),
        "{}",
        ran.all()
    );
}

#[test]
fn a_login_whose_exchange_answer_flare_cannot_read_hands_the_sign_in_to_a_person() {
    let api = FakeApi::start();
    let home = Home::new();
    let expected = json!({
        "kind": "after",
        "repair": "check that the server runs flare with the share API",
        "then": {"kind": "terminal", "call": format!("flare login {}", api.origin)},
    });
    for status in [502, 200] {
        api.route("POST", "/api/tokens/exchange", status, &json!("lost"));
        let ran = home.sharing(&["login", &api.origin, "--json"]);
        let printed = failure(&ran, "flare::network::bad_answer");
        assert_eq!(printed["recovery"], expected, "{status}: {}", ran.all());
    }
    api.route_raw("POST", "/api/tokens/exchange", 200, "", 0);
    let ran = home.sharing(&["login", &api.origin, "--json"]);
    let printed = failure(&ran, "flare::network::bad_answer");
    assert_eq!(printed["recovery"], expected, "empty: {}", ran.all());
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

#[test]
fn a_dry_run_of_update_checks_the_id_before_the_body_as_the_server_does() {
    let home = Home::new();
    let html = home.path().join("page.html");
    write(&html, "<p>x</p>\n");
    home.sharing(&[
        "share",
        html.to_str().unwrap(),
        "--update",
        "not an id",
        "--dry-run",
    ])
    .assert_failed("flare::shares::not_found");
    home.sharing(&[
        "share",
        html.to_str().unwrap(),
        "--update",
        FLARE,
        "--dry-run",
    ])
    .assert_failed("flare::shares::kind_not_served");
}

/// The failure document that a `--json` run printed, after asserting that
/// the run failed with `code`.
fn failure(ran: &run::Ran, code: &str) -> Value {
    ran.assert_failed(code);
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(printed["schema"], "flare/failure/v1", "{}", ran.all());
    assert_eq!(printed["code"], code, "{}", ran.all());
    printed
}

/// The `help:` line that a text run printed on standard error.
fn help_line(ran: &run::Ran) -> String {
    ran.stderr
        .lines()
        .find(|line| line.trim_start().starts_with("help:"))
        .unwrap_or_else(|| panic!("no help line: {}", ran.all()))
        .to_owned()
}

/// A share id that the server names in a partly shared refusal.
const PARTLY: &str = "dddddddddddddddddddddddddddddddd";

/// A partly shared refusal of `POST /api/shares`, naming `share` when given.
fn partly_shared(api: &FakeApi, share: Option<&str>) {
    let mut refusal = json!({
        "type": "about:blank",
        "title": "The share was created, and some of its grants or responses were not written.",
        "status": 409,
        "code": "flare::shares::partly_shared",
        "recovery": {"kind": "hint", "text": "run flare grant with the share id, --with and --re to finish"},
    });
    if let Some(share) = share {
        refusal["share"] = json!(share);
    }
    api.route("POST", "/api/shares", 409, &refusal);
}

#[test]
fn a_server_that_is_not_flare_fails_with_bad_answer_whose_recovery_repairs_then_retries() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route("GET", "/api/shares", 502, &json!("lost"));
    let ran = home.sharing(&["shares", "--json"]);
    let printed = failure(&ran, "flare::network::bad_answer");
    assert_eq!(
        printed["recovery"],
        json!({
            "kind": "after",
            "repair": "check that the server runs flare with the share API",
            "then": {"kind": "retry"},
        }),
        "{}",
        ran.all()
    );
    api.route("GET", "/api/shares", 502, &json!("lost"));
    let ran = home.sharing(&["shares"]);
    ran.assert_failed("flare::network::bad_answer");
    assert!(
        help_line(&ran).contains(
            "check that the server runs flare with the share API, then run the same command again"
        ),
        "{}",
        ran.all()
    );
}

/// The repair of a write whose 2xx answer flare could not read.
const CHECK_SHARES: &str = "check that flare shares does not already list the share or the version this command meant to make";

/// The recovery of a write whose 2xx answer flare could not read: check
/// `flare shares` before running the command again.
fn check_shares() -> Value {
    json!({"kind": "after", "repair": CHECK_SHARES, "then": {"kind": "retry"}})
}

/// The recovery of an answer whose next run starts clean.
fn server_check() -> Value {
    json!({
        "kind": "after",
        "repair": "check that the server runs flare with the share API",
        "then": {"kind": "retry"},
    })
}

/// A logged-in home with `review.md`, and the file's path.
fn with_file(api: &FakeApi) -> (Home, String) {
    let home = logged_in(api);
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    let file = file.to_str().unwrap().to_owned();
    (home, file)
}

#[test]
fn a_share_whose_success_answer_is_not_json_tells_the_caller_to_check_flare_shares_first() {
    let api = FakeApi::start();
    let (home, file) = with_file(&api);
    api.route("POST", "/api/shares", 200, &json!("lost"));
    let ran = home.sharing(&["share", &file, "--json"]);
    let printed = failure(&ran, "flare::network::bad_answer");
    assert_eq!(printed["recovery"], check_shares(), "{}", ran.all());

    api.route("POST", "/api/shares", 200, &json!("lost"));
    let ran = home.sharing(&["share", &file]);
    ran.assert_failed("flare::network::bad_answer");
    assert!(
        help_line(&ran).contains(&format!("{CHECK_SHARES}, then run the same command again")),
        "{}",
        ran.all()
    );
}

#[test]
fn a_new_version_whose_success_answer_is_not_json_tells_the_caller_to_check_flare_shares_first() {
    let api = FakeApi::start();
    let (home, file) = with_file(&api);
    api.route(
        "POST",
        &format!("/api/shares/{PARTLY}/versions"),
        200,
        &json!("lost"),
    );
    let ran = home.sharing(&["share", &file, "--update", PARTLY, "--json"]);
    let printed = failure(&ran, "flare::network::bad_answer");
    assert_eq!(printed["recovery"], check_shares(), "{}", ran.all());
}

#[test]
fn a_share_whose_success_answer_is_cut_short_or_empty_tells_the_caller_to_check_flare_shares_first()
{
    let api = FakeApi::start();
    let (home, file) = with_file(&api);
    for (body, length) in [("{\"share\":", 64), ("", 0)] {
        api.route_raw("POST", "/api/shares", 200, body, length);
        let ran = home.sharing(&["share", &file, "--json"]);
        let printed = failure(&ran, "flare::network::bad_answer");
        assert_eq!(
            printed["recovery"],
            check_shares(),
            "{body:?}: {}",
            ran.all()
        );
    }
}

#[test]
fn a_share_or_a_new_version_whose_request_gets_no_answer_tells_the_caller_to_check_flare_shares_first()
 {
    let api = FakeApi::start();
    let (home, file) = with_file(&api);
    api.route_silent("POST", "/api/shares");
    let ran = home.sharing(&["share", &file, "--json"]);
    let printed = failure(&ran, "flare::network::unreachable");
    assert_eq!(printed["recovery"], check_shares(), "{}", ran.all());

    api.route_silent("POST", &format!("/api/shares/{PARTLY}/versions"));
    let ran = home.sharing(&["share", &file, "--update", PARTLY, "--json"]);
    let printed = failure(&ran, "flare::network::unreachable");
    assert_eq!(printed["recovery"], check_shares(), "{}", ran.all());
}

#[test]
fn a_listing_or_a_grant_whose_request_gets_no_answer_keeps_retry() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route_silent("GET", "/api/shares");
    let ran = home.sharing(&["shares", "--json"]);
    let printed = failure(&ran, "flare::network::unreachable");
    assert_eq!(
        printed["recovery"],
        json!({"kind": "retry"}),
        "{}",
        ran.all()
    );

    api.route_silent("POST", &format!("/api/grants/{PARTLY}"));
    let ran = home.sharing(&["grant", PARTLY, "--with", "ana@example.org", "--json"]);
    let printed = failure(&ran, "flare::network::unreachable");
    assert_eq!(
        printed["recovery"],
        json!({"kind": "retry"}),
        "{}",
        ran.all()
    );
}

#[test]
fn a_login_whose_token_store_is_unavailable_hands_the_sign_in_to_a_person() {
    let api = FakeApi::start();
    let home = Home::new();
    api.route(
        "POST",
        "/api/tokens/exchange",
        503,
        &problem("flare::tokens::unavailable", 503, &api.origin),
    );
    let ran = home.sharing(&["login", &api.origin, "--json"]);
    let printed = failure(&ran, "flare::tokens::unavailable");
    assert_eq!(
        printed["recovery"],
        json!({"kind": "terminal", "call": format!("flare login {}", api.origin)}),
        "{}",
        ran.all()
    );
}

#[test]
fn a_listing_whose_token_store_is_unavailable_keeps_retry() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/shares",
        503,
        &problem("flare::tokens::unavailable", 503, &api.origin),
    );
    let ran = home.sharing(&["shares", "--json"]);
    let printed = failure(&ran, "flare::tokens::unavailable");
    assert_eq!(
        printed["recovery"],
        json!({"kind": "retry"}),
        "{}",
        ran.all()
    );
}

#[test]
fn a_share_refused_without_a_problem_document_keeps_the_server_check_and_retry() {
    let api = FakeApi::start();
    let (home, file) = with_file(&api);
    api.route("POST", "/api/shares", 502, &json!("lost"));
    let ran = home.sharing(&["share", &file, "--json"]);
    let printed = failure(&ran, "flare::network::bad_answer");
    assert_eq!(printed["recovery"], server_check(), "{}", ran.all());
}

#[test]
fn a_grant_or_a_listing_whose_success_answer_is_not_json_keeps_the_server_check_and_retry() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "POST",
        &format!("/api/grants/{PARTLY}"),
        200,
        &json!("lost"),
    );
    let ran = home.sharing(&["grant", PARTLY, "--with", "ana@example.org", "--json"]);
    let printed = failure(&ran, "flare::network::bad_answer");
    assert_eq!(printed["recovery"], server_check(), "{}", ran.all());

    api.route("GET", "/api/shares", 200, &json!("lost"));
    let ran = home.sharing(&["shares", "--json"]);
    let printed = failure(&ran, "flare::network::bad_answer");
    assert_eq!(printed["recovery"], server_check(), "{}", ran.all());
}

#[test]
fn a_partly_shared_refusal_names_flare_grant_with_the_share_the_server_named() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    let file = file.to_str().unwrap();
    partly_shared(&api, Some(PARTLY));
    let ran = home.sharing(&["share", file, "--with", "ana@example.org", "--json"]);
    let printed = failure(&ran, "flare::shares::partly_shared");
    let call = format!("flare grant {PARTLY} --with <ADDRESS> --re <ID>");
    assert_eq!(
        printed["recovery"],
        json!({"kind": "run", "call": call}),
        "{}",
        ran.all()
    );
    partly_shared(&api, Some(PARTLY));
    let ran = home.sharing(&["share", file, "--with", "ana@example.org"]);
    ran.assert_failed("flare::shares::partly_shared");
    assert!(help_line(&ran).contains(&call), "{}", ran.all());

    api.route(
        "POST",
        &format!("/api/grants/{PARTLY}"),
        200,
        &json!({
            "share": summary(PARTLY, "review"),
            "granted": ["ana@example.org"],
            "responds_to": [],
        }),
    );
    home.sharing(&["grant", PARTLY, "--with", "ana@example.org"])
        .assert_ok();
}

#[test]
fn a_partly_shared_refusal_without_a_share_names_flare_grant_with_a_placeholder_id() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("review.md");
    write(&file, "# Review\n");
    for share in [None, Some("not a share id")] {
        partly_shared(&api, share);
        let ran = home.sharing(&["share", file.to_str().unwrap(), "--json"]);
        let printed = failure(&ran, "flare::shares::partly_shared");
        assert_eq!(
            printed["recovery"],
            json!({"kind": "run", "call": "flare grant <ID> --with <ADDRESS> --re <ID>"}),
            "{share:?}: {}",
            ran.all()
        );
    }
}

/// The recovery and the alternative of a bad cursor to `flare activity`
/// with `flares`, the ids as the command line gave them.
fn assert_bad_cursor(ran: &run::Ran, flares: &str) {
    let printed = failure(ran, "flare::activity::bad_cursor");
    assert_eq!(
        printed["recovery"],
        json!({"kind": "run", "call": format!("flare activity {flares}")}),
        "{}",
        ran.all()
    );
    assert_eq!(
        printed["alternatives"],
        json!([{
            "kind": "after",
            "repair": "find the cursor that the last flare activity printed",
            "then": {"kind": "run", "call": format!("flare activity {flares} --since <CURSOR>")},
        }]),
        "{}",
        ran.all()
    );
}

/// A second flare's id.
const OTHER: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

#[test]
fn a_cursor_flare_did_not_write_fails_with_bad_cursor_and_names_flare_activity_with_the_flares() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let ran = home.sharing(&["activity", OTHER, FLARE, "--since", "nonsense", "--json"]);
    assert_bad_cursor(&ran, &format!("{OTHER} {FLARE}"));
    assert!(api.requests_to("/api/activity").is_empty());
    let ran = home.sharing(&["activity", OTHER, FLARE, "--since", "nonsense"]);
    ran.assert_failed("flare::activity::bad_cursor");
    assert!(
        help_line(&ran).contains(&format!("flare activity {OTHER} {FLARE}")),
        "{}",
        ran.all()
    );
}

#[test]
fn a_cursor_the_server_refuses_fails_with_bad_cursor_and_names_flare_activity_with_the_flare() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let mut refusal = problem("flare::activity::bad_cursor", 400, &api.origin);
    refusal["recovery"] = json!({
        "kind": "hint",
        "text": "pass the cursor that the last flare activity printed, or none to start over",
    });
    api.route("GET", "/api/activity", 400, &refusal);
    let ran = home.sharing(&["activity", FLARE, "--since", "fc1.AAAAAAAAAAA", "--json"]);
    assert_bad_cursor(&ran, FLARE);
}

/// Answers the next activity call with one response and `cursor`.
fn one_page(api: &FakeApi, cursor: &str, more: bool) {
    api.route(
        "GET",
        "/api/activity",
        200,
        &json!({
            "items": [response("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "First")],
            "cursor": cursor,
            "more": more,
        }),
    );
}

#[test]
fn activity_names_the_next_call_with_the_flares_and_the_last_cursor_under_next() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    one_page(&api, "fc1.AAAAAAAAAC8", false);
    let ran = home.sharing(&["activity", FLARE, "--json"]);
    ran.assert_ok();
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        printed["next"],
        json!([{
            "text": "Read the responses that arrive after these.",
            "commands": [{"call": format!("flare activity {FLARE} --since fc1.AAAAAAAAAC8")}],
        }]),
        "{}",
        ran.all()
    );
    assert_eq!(
        printed["summary"],
        json!({"cursor": "fc1.AAAAAAAAAC8", "more": false}),
        "{}",
        ran.all()
    );

    let other = OTHER;
    one_page(&api, "fc1.AAAAAAAAAD0", true);
    let ran = home.sharing(&["activity", other, FLARE, "--json"]);
    ran.assert_ok();
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        printed["next"],
        json!([{
            "text": "Read the responses that wait past this page.",
            "commands": [{"call": format!("flare activity {other} {FLARE} --since fc1.AAAAAAAAAD0")}],
        }]),
        "{}",
        ran.all()
    );
    assert_eq!(
        printed["summary"],
        json!({"cursor": "fc1.AAAAAAAAAD0", "more": true}),
        "{}",
        ran.all()
    );
}

#[test]
fn activity_in_text_prints_the_next_call_on_standard_error_and_no_summary_line() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    one_page(&api, "fc1.AAAAAAAAAC8", false);
    let ran = home.sharing(&["activity", FLARE]);
    ran.assert_ok();
    assert!(
        ran.stderr.contains(&format!(
            "Next: Read the responses that arrive after these.\n  flare activity {FLARE} --since fc1.AAAAAAAAAC8\n"
        )),
        "{}",
        ran.all()
    );
    assert!(!ran.stdout.contains("Next: --since"), "{}", ran.all());
}

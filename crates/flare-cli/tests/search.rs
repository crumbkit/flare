//! `flare search` against a fake share API: what it sends, what it prints,
//! and how a query with an error prints a caret line under each error and
//! exits with 2 before any request. Each refusal has an admitted twin.
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

use api::{FLARE, FakeApi, Fakes as _};
use run::Home;
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
        "role": "reader",
        "version": 2,
        "created": "2026-11-23T10:00:00Z",
    })
}

/// A search answer with one share titled `title`.
fn answer(query: &str, title: &str, partial: bool) -> Value {
    json!({
        "query": query,
        "parsed": [],
        "results": [summary(FLARE, title)],
        "next": null,
        "partial": partial,
    })
}

/// A problem document of a search that did not run, with one error.
fn search_problem(code: &str, error: &str, start: u32, end: u32) -> Value {
    json!({
        "type": "about:blank",
        "title": "refused",
        "status": 422,
        "code": code,
        "recovery": {"kind": "hint", "text": "fix the search"},
        "errors": [{
            "code": error,
            "span": {"start": start, "end": end},
            "message": "Nobody called nobody is known here; use me, an email address or a name this server knows.",
        }],
    })
}

#[test]
fn search_sends_the_query_and_prints_each_share_as_flare_shares_prints_it() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/search",
        200,
        &answer("budget from:me", "Budget plan", false),
    );
    let ran = home.sharing(&["search", "budget  from:me,ana@example.org"]);
    ran.assert_ok();
    assert!(ran.stdout.contains("Budget plan"), "{}", ran.all());
    assert!(ran.stdout.contains(FLARE), "{}", ran.all());
    assert!(ran.stdout.contains("reader"), "{}", ran.all());
    assert_eq!(
        api.requests_to("/api/search")[0].target,
        "/api/search?q=budget++from%3Ame%2Cana%40example.org&limit=51"
    );
}

#[test]
fn search_json_prints_the_rows_with_the_canonical_query_and_whether_it_is_partial() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/search",
        200,
        &answer("budget", "Budget plan", true),
    );
    let ran = home.sharing(&["search", "budget", "--json"]);
    ran.assert_ok();
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(printed["schema"], "flare/search/v1", "{}", ran.all());
    assert_eq!(printed["data"][0]["title"], "Budget plan");
    assert_eq!(printed["summary"]["query"], "budget");
    assert_eq!(printed["summary"]["partial"], true);
}

#[test]
fn a_partial_search_says_so_in_text_and_a_whole_one_does_not() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/search",
        200,
        &answer("budget", "Budget plan", true),
    );
    api.route(
        "GET",
        "/api/search",
        200,
        &answer("budget", "Budget plan", false),
    );
    let partial = home.sharing(&["search", "budget"]);
    partial.assert_ok();
    assert!(
        partial.stdout.contains("may be missing"),
        "{}",
        partial.all()
    );
    let whole = home.sharing(&["search", "budget"]);
    whole.assert_ok();
    assert!(!whole.all().contains("may be missing"), "{}", whole.all());
}

#[test]
fn search_all_and_limit_ask_the_server_for_that_many_results() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route("GET", "/api/search", 200, &answer("x", "X", false));
    api.route("GET", "/api/search", 200, &answer("x", "X", false));
    home.sharing(&["search", "x", "--all"]).assert_ok();
    home.sharing(&["search", "x", "--limit", "7"]).assert_ok();
    let targets: Vec<String> = api
        .requests_to("/api/search")
        .into_iter()
        .map(|request| request.target)
        .collect();
    assert_eq!(
        targets,
        ["/api/search?q=x&limit=500", "/api/search?q=x&limit=8"]
    );
}

#[test]
fn search_everyone_asks_for_shares_open_to_everyone_and_a_plain_search_does_not() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route("GET", "/api/search", 200, &answer("x", "X", false));
    api.route("GET", "/api/search", 200, &answer("x", "X", false));
    home.sharing(&["search", "x", "--everyone"]).assert_ok();
    home.sharing(&["search", "x"]).assert_ok();
    let targets: Vec<String> = api
        .requests_to("/api/search")
        .into_iter()
        .map(|request| request.target)
        .collect();
    assert_eq!(
        targets,
        [
            "/api/search?q=x&limit=51&everyone=true",
            "/api/search?q=x&limit=51"
        ]
    );
}

/// A search answer with `count` shares.
fn answer_of(count: usize) -> Value {
    let results: Vec<Value> = (0..count)
        .map(|index| summary(&format!("{FLARE}{index}"), &format!("Share {index}")))
        .collect();
    json!({"query": "x", "parsed": [], "results": results, "next": null, "partial": false})
}

#[test]
fn a_limit_that_cuts_the_results_prints_a_footer_and_one_that_does_not_prints_none() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route("GET", "/api/search", 200, &answer_of(3));
    api.route("GET", "/api/search", 200, &answer_of(2));
    let cut = home.sharing(&["search", "x", "--limit", "2"]);
    cut.assert_ok();
    assert!(cut.all().contains("--limit"), "{}", cut.all());
    assert!(!cut.stdout.contains("Share 2"), "{}", cut.all());
    let whole = home.sharing(&["search", "x", "--limit", "2"]);
    whole.assert_ok();
    assert!(!whole.all().contains("--limit"), "{}", whole.all());
}

#[test]
fn a_query_that_starts_with_a_minus_is_a_query_and_not_a_flag() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route("GET", "/api/search", 200, &answer("-draft", "Plan", false));
    api.route(
        "GET",
        "/api/search",
        200,
        &answer("-from:me", "Plan", false),
    );
    home.sharing(&["search", "-draft"]).assert_ok();
    home.sharing(&["search", "-from:me"]).assert_ok();
    let targets: Vec<String> = api
        .requests_to("/api/search")
        .into_iter()
        .map(|request| request.target)
        .collect();
    assert_eq!(
        targets,
        [
            "/api/search?q=-draft&limit=51",
            "/api/search?q=-from%3Ame&limit=51"
        ]
    );
}

#[test]
fn a_query_with_errors_prints_a_caret_line_under_each_and_exits_2_without_a_request() {
    let home = Home::new();
    let ran = home.run(&["search", "form:erin \"open"]);
    assert_eq!(ran.code, Some(2), "{}", ran.all());
    assert!(
        ran.stderr.contains("flare::search::invalid"),
        "{}",
        ran.all()
    );
    assert!(
        ran.stderr.contains("cause: form:erin \"open\n"),
        "{}",
        ran.all()
    );
    assert!(
        ran.stderr
            .contains("cause: ^^^^ There is no filter called form; did you mean from?"),
        "{}",
        ran.all()
    );
    assert!(
        ran.stderr
            .contains("cause:           ^^^^^ This quote is not closed"),
        "{}",
        ran.all()
    );
    assert!(!ran.all().contains("flare::login::needed"), "{}", ran.all());

    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/search",
        200,
        &answer("from:erin", "Plan", false),
    );
    home.sharing(&["search", "from:erin \"open\""]).assert_ok();
}

#[test]
fn a_type_flare_does_not_serve_fails_before_a_request_and_markdown_is_sent() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let ran = home.sharing(&["search", "type:pdf"]);
    assert_eq!(ran.code, Some(2), "{}", ran.all());
    assert!(
        ran.stderr.contains("flare::search::invalid"),
        "{}",
        ran.all()
    );
    assert!(
        ran.stderr.contains("There is no type called pdf"),
        "{}",
        ran.all()
    );
    assert!(api.requests_to("/api/search").is_empty());
    api.route(
        "GET",
        "/api/search",
        200,
        &answer("type:markdown", "Plan", false),
    );
    home.sharing(&["search", "type:markdown"]).assert_ok();
}

#[test]
fn a_person_the_server_does_not_know_prints_its_caret_line_and_exits_1() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    api.route(
        "GET",
        "/api/search",
        422,
        &search_problem(
            "flare::search::unknown_person",
            "flare::search::unknown_person",
            5,
            11,
        ),
    );
    let ran = home.sharing(&["search", "from:nobody"]);
    ran.assert_failed("flare::search::unknown_person");
    assert_eq!(ran.code, Some(1), "{}", ran.all());
    assert!(ran.stderr.contains("cause: from:nobody\n"), "{}", ran.all());
    assert!(
        ran.stderr
            .contains("cause:      ^^^^^^ Nobody called nobody"),
        "{}",
        ran.all()
    );
    api.route(
        "GET",
        "/api/search",
        200,
        &answer("from:ana", "Plan", false),
    );
    home.sharing(&["search", "from:ana"]).assert_ok();
}

#[test]
fn a_valid_search_without_a_login_names_flare_login() {
    let home = Home::new();
    let ran = home.sharing(&["search", "budget"]);
    ran.assert_failed("flare::login::needed");
}

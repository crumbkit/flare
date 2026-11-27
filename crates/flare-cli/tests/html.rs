//! `flare share` with an HTML file: the form it sends, the checks it makes
//! before any request, and its dry run. Each refusal has a twin that shows
//! the admitted case.
#![cfg(test)]

#[path = "support/api.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the fake API is test support, and these tests use part of it"
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
use run::{Home, write};
use serde_json::{Value, json};
use std::path::PathBuf;

const PAGE: &str = "<!doctype html><title>Plan</title><p>hello</p>\n";

fn logged_in(api: &FakeApi) -> Home {
    let home = Home::new();
    api.issue();
    home.sharing(&["login", &api.origin]).assert_ok();
    home
}

fn summary(version: u32) -> Value {
    json!({
        "id": FLARE,
        "title": "plan",
        "format": "html",
        "owner": "ana@example.org",
        "role": "owner",
        "version": version,
        "created": "2026-11-23T10:00:00Z",
    })
}

fn page(home: &Home, name: &str, text: &str) -> PathBuf {
    let file = home.path().join(name);
    write(&file, text);
    file
}

fn parts(body: &str) -> Vec<String> {
    let boundary = body
        .lines()
        .next()
        .unwrap()
        .trim_start_matches("--")
        .to_owned();
    body.split(&format!("--{boundary}"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn share_sends_an_html_file_as_a_form_with_an_html_part_and_an_html_share_part() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = page(&home, "plan.html", PAGE);
    api.route(
        "POST",
        "/api/shares",
        201,
        &json!({"share": summary(1), "granted": [], "responds_to": []}),
    );
    home.sharing(&["share", file.to_str().unwrap(), "--with", "bob@example.org"])
        .assert_ok();
    let sent = &api.requests_to("/api/shares")[0];
    assert!(sent.headers["content-type"].starts_with("multipart/form-data; boundary="));
    let parts = parts(&sent.body);
    let share: Value =
        serde_json::from_str(parts[1].split("\r\n\r\n").nth(1).unwrap().trim_end()).unwrap();
    assert_eq!(share["format"], "html");
    assert_eq!(share["content"], "");
    assert_eq!(share["title"], "plan");
    assert!(parts[2].contains("name=\"html\""), "{}", parts[2]);
    assert!(parts[2].contains("Content-Type: text/html"), "{}", parts[2]);
    assert!(
        parts[2].ends_with(&format!("\r\n\r\n{PAGE}\r\n")),
        "{:?}",
        parts[2]
    );
}

#[test]
fn share_update_sends_an_html_file_to_the_versions_route() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = page(&home, "plan.htm", PAGE);
    api.route(
        "POST",
        &format!("/api/shares/{FLARE}/versions"),
        201,
        &summary(2),
    );
    home.sharing(&["share", file.to_str().unwrap(), "--update", FLARE])
        .assert_ok();
    let sent = &api.requests_to(&format!("/api/shares/{FLARE}/versions"))[0];
    assert!(sent.body.contains("name=\"html\""), "{}", sent.body);
}

#[test]
fn share_refuses_an_empty_an_oversized_and_a_non_utf8_html_file_before_any_request() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let empty = page(&home, "empty.html", "");
    let big = page(&home, "big.html", &"a".repeat(4 * 1024 * 1024 + 1));
    let binary = home.path().join("binary.html");
    #[expect(
        clippy::disallowed_methods,
        reason = "a test writes its fixture files directly"
    )]
    std::fs::write(&binary, [0xff_u8, 0xfe, b'<']).unwrap();
    let cases: [(&PathBuf, &str); 3] = [
        (&empty, "flare::shares::invalid"),
        (&big, "flare::html::too_large"),
        (&binary, "flare::html::not_utf8"),
    ];
    for (file, code) in cases {
        home.sharing(&["share", file.to_str().unwrap()])
            .assert_failed(code);
    }
    assert!(api.requests_to("/api/shares").is_empty());
    let exact = page(&home, "exact.html", &"a".repeat(4 * 1024 * 1024));
    api.route(
        "POST",
        "/api/shares",
        201,
        &json!({"share": summary(1), "granted": [], "responds_to": []}),
    );
    home.sharing(&["share", exact.to_str().unwrap()])
        .assert_ok();
}

#[test]
fn a_dry_run_of_an_html_file_checks_it_and_writes_it_unchanged_with_out() {
    let home = Home::new();
    let file = page(&home, "plan.html", PAGE);
    let out = home.path().join("out.html");
    home.sharing(&[
        "share",
        file.to_str().unwrap(),
        "--dry-run",
        "--out",
        out.to_str().unwrap(),
    ])
    .assert_ok();
    assert_eq!(std::fs::read_to_string(&out).unwrap(), PAGE);
    let bad = page(&home, "empty.html", "");
    home.sharing(&["share", bad.to_str().unwrap(), "--dry-run"])
        .assert_failed("flare::shares::invalid");
}

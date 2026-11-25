//! The title `flare share` gives a new flare: the text of the document's
//! first level-one heading when `--title` is absent, and the file name
//! when the document has no such heading. Each fallback has a twin that
//! shows the heading used.
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

/// A home whose person is logged in to `api`.
fn logged_in(api: &FakeApi) -> Home {
    let home = Home::new();
    api.issue();
    home.sharing(&["login", &api.origin]).assert_ok();
    home
}

/// A share summary as the API sends it.
fn summary(title: &str) -> Value {
    json!({
        "id": FLARE,
        "title": title,
        "format": "markdown",
        "owner": "ana@example.org",
        "role": "owner",
        "version": 1,
        "created": "2026-11-23T10:00:00Z",
    })
}

/// The title of the share that `flare share` creates from `content` in a
/// file named `name`, with `extra` arguments.
fn sent_title(name: &str, content: &str, extra: &[&str]) -> Value {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join(name);
    write(&file, content);
    api.route(
        "POST",
        "/api/shares",
        201,
        &json!({"share": summary("sent"), "granted": [], "responds_to": []}),
    );
    let mut args = vec!["share", file.to_str().unwrap(), "--json"];
    args.extend_from_slice(extra);
    home.sharing(&args).assert_ok();
    api.requests_to("/api/shares")[0].json()["title"].clone()
}

#[test]
fn a_document_that_starts_with_a_level_one_heading_is_shared_with_the_heading_as_its_title() {
    assert_eq!(
        sent_title("plan.md", "# Quarterly plan\n\nText.\n", &[]),
        "Quarterly plan"
    );
}

#[test]
fn a_title_given_with_title_wins_over_the_heading() {
    assert_eq!(
        sent_title("plan.md", "# Quarterly plan\n", &["--title", "Other"]),
        "Other"
    );
}

#[test]
fn a_document_with_no_level_one_heading_takes_the_file_name_and_one_with_a_heading_does_not() {
    assert_eq!(
        sent_title("plan.md", "## Background\n\nText.\n", &[]),
        "plan"
    );
    assert_eq!(sent_title("plan.md", "#\n\nText.\n", &[]), "plan");
    assert_eq!(sent_title("plan.md", "# Named\n\nText.\n", &[]), "Named");
}

#[test]
fn a_heading_inside_a_code_block_is_ignored_and_a_setext_heading_is_used() {
    assert_eq!(
        sent_title("plan.md", "```\n# Not a title\n```\n", &[]),
        "plan"
    );
    assert_eq!(
        sent_title("plan.md", "Setext title\n===\n", &[]),
        "Setext title"
    );
}

#[test]
fn the_title_is_the_headings_text_without_its_markup() {
    assert_eq!(
        sent_title("plan.md", "# The *new* `api` [guide](x)\n", &[]),
        "The new api guide"
    );
}

#[test]
fn a_long_heading_of_multi_byte_characters_is_cut_to_at_most_200_bytes_on_a_character_boundary() {
    let title = sent_title("plan.md", &format!("# {}\n", "é".repeat(150)), &[]);
    let title = title.as_str().unwrap();
    assert!(title.len() <= 200, "{}", title.len());
    assert_eq!(title, "é".repeat(100));
}

#[test]
fn an_html_document_keeps_its_file_name_as_title() {
    assert_eq!(sent_title("page.html", "<h1>Heading</h1>\n", &[]), "page");
}

#[test]
fn a_dry_run_reports_the_title_it_would_use_from_the_heading() {
    let home = Home::new();
    let file = home.path().join("plan.md");
    write(&file, "# Quarterly plan\n");
    let ran = home.sharing(&["share", file.to_str().unwrap(), "--dry-run", "--json"]);
    ran.assert_ok();
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(printed["data"]["title"], "Quarterly plan", "{}", ran.all());
    assert!(
        printed["data"]["checks"]
            .to_string()
            .contains("the title \\\"Quarterly plan\\\""),
        "{}",
        ran.all()
    );
}

#[test]
fn an_update_sends_no_title_so_a_changed_heading_does_not_rename_the_flare() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = home.path().join("plan.md");
    write(&file, "# A new heading\n");
    let mut updated = summary("Quarterly plan");
    updated["version"] = json!(2);
    api.route(
        "POST",
        &format!("/api/shares/{FLARE}/versions"),
        201,
        &updated,
    );
    let ran = home.sharing(&["share", file.to_str().unwrap(), "--update", FLARE, "--json"]);
    ran.assert_ok();
    let sent = &api.requests_to(&format!("/api/shares/{FLARE}/versions"))[0];
    assert_eq!(
        sent.json(),
        json!({"format": "markdown", "content": "# A new heading\n"})
    );
    let printed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(printed["data"]["title"], "Quarterly plan", "{}", ran.all());
}

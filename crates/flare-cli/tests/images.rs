//! `flare share` with a Markdown file that shows images: which images it
//! reads, how it sends them, and the two refusals it makes before any
//! request. Each refusal has a twin that shows the admitted case.
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
use std::path::{Path, PathBuf};

/// A home whose person is logged in to `api`.
fn logged_in(api: &FakeApi) -> Home {
    let home = Home::new();
    api.issue();
    home.sharing(&["login", &api.origin]).assert_ok();
    home
}

fn summary(id: &str, version: u32) -> Value {
    json!({
        "id": id,
        "title": "notes",
        "format": "markdown",
        "owner": "ana@example.org",
        "role": "owner",
        "version": version,
        "created": "2026-11-23T10:00:00Z",
    })
}

fn created(api: &FakeApi) {
    api.route(
        "POST",
        "/api/shares",
        201,
        &json!({"share": summary(FLARE, 1), "granted": [], "responds_to": []}),
    );
}

/// A `docs` folder holding `notes.md` with `markdown`, and the file.
fn document(home: &Home, markdown: &str) -> PathBuf {
    let folder = home.path().join("docs");
    std::fs::create_dir_all(folder.join("img")).unwrap();
    let file = folder.join("notes.md");
    write(&file, markdown);
    file
}

fn put(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write(path, text);
}

/// The value of the boundary in a form's Content-Type.
fn boundary_of(content_type: &str) -> String {
    content_type
        .strip_prefix("multipart/form-data; boundary=")
        .unwrap()
        .to_owned()
}

#[test]
fn share_sends_each_image_the_document_shows_as_a_form_part_named_by_its_path() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = document(
        &home,
        "# Notes\n\n![chart](img/chart.png)\n\n![web](https://example.org/x.png)\n\n![again](img/chart.png)\n![photo](photo.jpg)\n",
    );
    put(&file.with_file_name("img/chart.png"), "CHARTBYTES");
    put(&file.with_file_name("photo.jpg"), "PHOTOBYTES");
    created(&api);
    home.sharing(&["share", file.to_str().unwrap(), "--json"])
        .assert_ok();
    let sent = &api.requests_to("/api/shares")[0];
    let boundary = boundary_of(&sent.headers["content-type"]);
    let parts: Vec<&str> = sent.body.split(&format!("--{boundary}")).collect();
    assert_eq!(parts.len(), 5, "{}", sent.body);
    assert!(parts[1].contains("name=\"share\""));
    let share: Value =
        serde_json::from_str(parts[1].split("\r\n\r\n").nth(1).unwrap().trim_end()).unwrap();
    assert_eq!(share["title"], "notes");
    assert!(
        parts[2].contains("filename=\"img/chart.png\""),
        "{}",
        parts[2]
    );
    assert!(
        parts[2].ends_with("\r\n\r\nCHARTBYTES\r\n"),
        "{:?}",
        parts[2]
    );
    assert!(parts[3].contains("filename=\"photo.jpg\""), "{}", parts[3]);
    assert!(
        parts[3].ends_with("\r\n\r\nPHOTOBYTES\r\n"),
        "{:?}",
        parts[3]
    );
    assert_eq!(parts[4], "--\r\n");
}

#[test]
fn share_of_a_document_with_no_relative_image_sends_json() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = document(&home, "# Notes\n\n![web](https://example.org/x.png)\n");
    created(&api);
    home.sharing(&["share", file.to_str().unwrap()]).assert_ok();
    let sent = &api.requests_to("/api/shares")[0];
    assert_eq!(sent.headers["content-type"], "application/json");
}

#[test]
fn share_update_sends_its_images_to_the_versions_route() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = document(&home, "# Notes\n\n![chart](img/chart.png)\n");
    put(&file.with_file_name("img/chart.png"), "CHARTBYTES");
    api.route(
        "POST",
        &format!("/api/shares/{FLARE}/versions"),
        201,
        &summary(FLARE, 2),
    );
    home.sharing(&["share", file.to_str().unwrap(), "--update", FLARE])
        .assert_ok();
    let sent = &api.requests_to(&format!("/api/shares/{FLARE}/versions"))[0];
    assert!(sent.headers["content-type"].starts_with("multipart/form-data; boundary="));
    assert!(sent.body.contains("filename=\"img/chart.png\""));
}

#[test]
fn an_image_with_no_file_stops_share_and_its_dry_run_before_any_request_and_a_present_one_is_sent()
{
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = document(&home, "# Notes\n\n![chart](img/chart.png)\n");
    for args in [
        vec!["share", file.to_str().unwrap()],
        vec!["share", file.to_str().unwrap(), "--dry-run"],
    ] {
        let ran = home.sharing(&args);
        ran.assert_failed("flare::images::missing");
        assert!(ran.all().contains("img/chart.png"), "{}", ran.all());
    }
    assert!(api.requests_to("/api/shares").is_empty());
    put(&file.with_file_name("img/chart.png"), "CHARTBYTES");
    created(&api);
    home.sharing(&["share", file.to_str().unwrap()]).assert_ok();
    assert_eq!(api.requests_to("/api/shares").len(), 1);
}

#[test]
fn an_image_outside_the_documents_folder_by_a_parent_path_or_a_link_stops_share_before_any_request()
{
    let api = FakeApi::start();
    let home = logged_in(&api);
    put(&home.path().join("secret.png"), "SECRET");
    let parent = document(&home, "# Notes\n\n![x](../secret.png)\n");
    home.sharing(&["share", parent.to_str().unwrap()])
        .assert_failed("flare::images::outside_folder");
    let linked = document(&home, "# Notes\n\n![x](img/link.png)\n");
    std::os::unix::fs::symlink(
        home.path().join("secret.png"),
        linked.with_file_name("img/link.png"),
    )
    .unwrap();
    home.sharing(&["share", linked.to_str().unwrap()])
        .assert_failed("flare::images::outside_folder");
    assert!(api.requests_to("/api/shares").is_empty());
    put(&linked.with_file_name("img/inside.png"), "INSIDE");
    let inside = document(&home, "# Notes\n\n![x](img/inside.png)\n");
    created(&api);
    home.sharing(&["share", inside.to_str().unwrap()])
        .assert_ok();
    assert!(!api.requests_to("/api/shares")[0].body.contains("SECRET"));
}

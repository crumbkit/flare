//! `flare share` with a Markdown file that shows images: which images it
//! reads, how it sends them, how many it says it sent, and the refusals it
//! makes before any request. Each refusal has a twin that shows the
//! admitted case.
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
    put(&file.with_file_name("img/chart.png"), "GIF89aCHARTBYTES");
    put(&file.with_file_name("photo.jpg"), "GIF89aPHOTOBYTES");
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
        parts[2].ends_with("\r\n\r\nGIF89aCHARTBYTES\r\n"),
        "{:?}",
        parts[2]
    );
    assert!(parts[3].contains("filename=\"photo.jpg\""), "{}", parts[3]);
    assert!(
        parts[3].ends_with("\r\n\r\nGIF89aPHOTOBYTES\r\n"),
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
    put(&file.with_file_name("img/chart.png"), "GIF89aCHARTBYTES");
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
    put(&file.with_file_name("img/chart.png"), "GIF89aCHARTBYTES");
    created(&api);
    home.sharing(&["share", file.to_str().unwrap()]).assert_ok();
    assert_eq!(api.requests_to("/api/shares").len(), 1);
}

#[test]
fn an_image_outside_the_documents_folder_by_a_parent_path_or_a_link_stops_share_before_any_request()
{
    let api = FakeApi::start();
    let home = logged_in(&api);
    put(&home.path().join("secret.png"), "GIF89aSECRET");
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
    put(&linked.with_file_name("img/inside.png"), "GIF89aINSIDE");
    let inside = document(&home, "# Notes\n\n![x](img/inside.png)\n");
    created(&api);
    home.sharing(&["share", inside.to_str().unwrap()])
        .assert_ok();
    assert!(
        !api.requests_to("/api/shares")[0]
            .body
            .contains("GIF89aSECRET")
    );
}

#[test]
fn share_prints_how_many_images_it_sent_and_a_document_with_none_prints_zero() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let file = document(&home, "# Notes\n\n![a](img/a.png) ![b](b.png)\n");
    put(&file.with_file_name("img/a.png"), "GIF89aA");
    put(&file.with_file_name("b.png"), "GIF89aB");
    created(&api);
    let ran = home.sharing(&["share", file.to_str().unwrap(), "--json"]);
    ran.assert_ok();
    let printed: Value = serde_json::from_str(ran.stdout.trim()).unwrap();
    assert_eq!(printed["data"]["images"], 2, "{printed}");
    created(&api);
    let text = home.sharing(&["share", file.to_str().unwrap()]);
    text.assert_ok();
    assert!(text.stdout.contains("IMAGES"), "{}", text.stdout);
    let dry = home.sharing(&["share", file.to_str().unwrap(), "--dry-run", "--json"]);
    dry.assert_ok();
    let checked: Value = serde_json::from_str(dry.stdout.trim()).unwrap();
    assert_eq!(checked["data"]["images"], 2, "{checked}");
    let none = document(&home, "# Notes\n");
    created(&api);
    let ran = home.sharing(&["share", none.to_str().unwrap(), "--json"]);
    let printed: Value = serde_json::from_str(ran.stdout.trim()).unwrap();
    assert_eq!(printed["data"]["images"], 0, "{printed}");
}

/// Runs `flare share` on a document that shows `markdown` and asserts it
/// fails with `code` before any request.
fn refused_before_any_request(home: &Home, api: &FakeApi, markdown: &str, code: &str) {
    let file = document(home, markdown);
    for args in [
        vec!["share", file.to_str().unwrap()],
        vec!["share", file.to_str().unwrap(), "--dry-run"],
    ] {
        home.sharing(&args).assert_failed(code);
    }
    assert!(api.requests_to("/api/shares").is_empty(), "{markdown}");
}

#[test]
fn share_refuses_before_any_request_an_image_path_the_server_refuses_and_sends_its_plain_twin() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let folder = home.path().join("docs");
    put(&folder.join("img/chart.png"), "GIF89aCHART");
    put(&folder.join("my shot.png"), "GIF89aSHOT");
    for markdown in [
        "![x](./img/chart.png)\n",
        "![x](img/../img/chart.png)\n",
        "![x](<my shot.png>)\n",
    ] {
        refused_before_any_request(&home, &api, markdown, "flare::images::bad_path");
    }
    let absolute = format!("![x]({})\n", folder.join("img/chart.png").display());
    refused_before_any_request(&home, &api, &absolute, "flare::images::bad_path");
    created(&api);
    let plain = document(&home, "![x](img/chart.png)\n");
    home.sharing(&["share", plain.to_str().unwrap()])
        .assert_ok();
}

#[test]
fn share_refuses_before_any_request_more_than_ten_images_and_sends_ten() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let folder = home.path().join("docs");
    let mut markdown = String::new();
    for index in 0..11 {
        put(&folder.join(format!("{index}.png")), "GIF89aIMAGE");
        markdown.push_str(&format!("![x]({index}.png)\n"));
    }
    refused_before_any_request(&home, &api, &markdown, "flare::images::too_many");
    let ten: String = markdown
        .lines()
        .take(10)
        .map(|line| format!("{line}\n"))
        .collect();
    created(&api);
    home.sharing(&["share", document(&home, &ten).to_str().unwrap()])
        .assert_ok();
}

#[test]
fn share_refuses_before_any_request_an_image_over_5_mib_or_not_an_image_by_its_first_bytes() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let folder = home.path().join("docs");
    let large = format!("GIF89a{}", "\0".repeat(5 * 1024 * 1024));
    put(&folder.join("large.gif"), &large);
    refused_before_any_request(&home, &api, "![x](large.gif)\n", "flare::images::too_large");
    put(
        &folder.join("drawing.png"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    );
    refused_before_any_request(
        &home,
        &api,
        "![x](drawing.png)\n",
        "flare::images::format_refused",
    );
    put(&folder.join("photo.png"), "GIF89aPHOTO");
    created(&api);
    home.sharing(&[
        "share",
        document(&home, "![x](photo.png)\n").to_str().unwrap(),
    ])
    .assert_ok();
}

/// A GIF of `bytes` bytes in all.
fn gif_of(bytes: usize) -> String {
    format!("GIF89a{}", "\0".repeat(bytes - 6))
}

#[test]
fn share_refuses_before_any_request_an_image_over_the_upload_limit_naming_the_limit_and_the_file() {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let folder = home.path().join("docs");
    let limit = flare_core::MAX_UPLOAD_BYTES;
    put(&folder.join("shot.gif"), &gif_of(limit + limit / 8));
    let file = document(&home, "![x](shot.gif)\n");
    for args in [
        vec!["share", file.to_str().unwrap()],
        vec!["share", file.to_str().unwrap(), "--dry-run"],
    ] {
        let ran = home.sharing(&args);
        ran.assert_failed("flare::images::body_too_large");
        assert!(ran.all().contains("shot.gif"), "{}", ran.all());
        assert!(ran.all().contains("4 MiB"), "{}", ran.all());
    }
    assert!(api.requests_to("/api/shares").is_empty());
}

#[test]
fn share_refuses_before_any_request_images_that_together_pass_the_upload_limit_and_sends_smaller_ones()
 {
    let api = FakeApi::start();
    let home = logged_in(&api);
    let folder = home.path().join("docs");
    let limit = flare_core::MAX_UPLOAD_BYTES;
    put(&folder.join("a.gif"), &gif_of(limit / 2));
    put(&folder.join("b.gif"), &gif_of(limit / 2 + 1024));
    let file = document(&home, "![a](a.gif)\n![b](b.gif)\n");
    for args in [
        vec!["share", file.to_str().unwrap()],
        vec!["share", file.to_str().unwrap(), "--dry-run"],
    ] {
        let ran = home.sharing(&args);
        ran.assert_failed("flare::images::body_too_large");
        assert!(ran.all().contains("b.gif"), "{}", ran.all());
        assert!(ran.all().contains("4 MiB"), "{}", ran.all());
    }
    assert!(api.requests_to("/api/shares").is_empty());
    put(&folder.join("a.gif"), &gif_of(limit / 3));
    put(&folder.join("b.gif"), &gif_of(limit / 3));
    created(&api);
    home.sharing(&["share", file.to_str().unwrap()]).assert_ok();
    let sent = &api.requests_to("/api/shares")[0];
    assert!(sent.body.len() <= limit, "{}", sent.body.len());
}

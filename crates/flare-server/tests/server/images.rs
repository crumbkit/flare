//! A share sent with its images as a multipart form, its reading page,
//! and the file route that serves each image. Each refusal also shows the
//! admitted case.

use crate::api::Api;
use crate::harness::Reply;
use image::ImageEncoder;
use image::codecs::png::PngEncoder;

const BOUNDARY: &str = "flare-test-7d3c";
const DOC: &str = "# Chart\n\n![the chart](img/chart.png)\n";

/// A grey PNG `width` pixels wide and one high.
fn png(width: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let row = vec![128_u8; usize::try_from(width).unwrap()];
    PngEncoder::new(&mut bytes)
        .write_image(&row, width, 1, image::ExtendedColorType::L8)
        .unwrap();
    bytes
}

/// A form with the share `markdown` and one image part per `(path, bytes)`.
fn form(markdown: &str, images: &[(&str, &[u8])]) -> Vec<u8> {
    let share = serde_json::json!({
        "title": "Chart",
        "format": "markdown",
        "content": markdown,
        "with": ["bob@example.com"],
        "re": [],
    });
    let mut body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"share\"\r\nContent-Type: application/json\r\n\r\n{share}\r\n"
    )
    .into_bytes();
    for (path, bytes) in images {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"{path}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    body
}

async fn upload(api: &Api, token: &str, body: Vec<u8>) -> Reply {
    api.call("POST", "/api/shares", Some(token))
        .body(&format!("multipart/form-data; boundary={BOUNDARY}"), body)
        .send()
        .await
}

/// Alice's share of [`DOC`] with its chart, granted to bob: alice's token,
/// the share's id and the file route of the chart.
async fn shared(api: &Api) -> (String, String, String) {
    let token = api.token_for("alice").await;
    let reply = upload(api, &token, form(DOC, &[("img/chart.png", &png(8))])).await;
    assert_eq!(reply.status, 201, "{}", reply.body);
    let shared: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    let id = shared["share"]["id"].as_str().unwrap().to_owned();
    let bob = api.session("bob").await;
    let page = api.browser("GET", &format!("/s/{id}"), &bob).send().await;
    assert_eq!(page.status, 200, "{}", page.body);
    let prefix = format!("/api/shares/{id}/versions/1/files/");
    let start = page.body.find(&prefix).expect("the page shows the chart");
    let end = start + page.body[start..].find('"').unwrap();
    (token, id, page.body[start..end].to_owned())
}

#[tokio::test]
async fn a_share_sent_with_an_image_shows_it_and_the_file_route_serves_it_inline_in_a_sandbox() {
    let api = Api::start().await;
    let (token, _, route) = shared(&api).await;
    let bob = api.session("bob").await;
    for reply in [
        api.browser("GET", &route, &bob).send().await,
        api.call("GET", &route, Some(&token)).send().await,
    ] {
        assert_eq!(reply.status, 200, "{}", reply.body);
        assert_eq!(reply.header("content-type"), Some("image/png"));
        assert_eq!(reply.header("x-content-type-options"), Some("nosniff"));
        assert_eq!(
            reply.header("content-security-policy"),
            Some("default-src 'none'; sandbox")
        );
        assert_eq!(reply.header("cache-control"), Some("private, no-store"));
        assert_eq!(reply.header("content-disposition"), Some("inline"));
        assert!(reply.body.starts_with("\u{fffd}PNG"));
    }
}

#[tokio::test]
async fn a_reader_with_no_grant_an_unlisted_file_and_a_missing_version_get_the_same_404() {
    let api = Api::start().await;
    let (_, id, route) = shared(&api).await;
    let carol = api.token_for("carol").await;
    let alice = api.token_for("alice").await;
    let unlisted = format!("/api/shares/{id}/versions/1/files/{}", "ab".repeat(16));
    let missing = route.replace("/versions/1/", "/versions/2/");
    for (token, path) in [(&carol, &route), (&alice, &unlisted), (&alice, &missing)] {
        let reply = api.call("GET", path, Some(token)).send().await;
        assert_eq!(reply.status, 404, "{path}");
        assert_eq!(reply.code().as_deref(), Some("flare::shares::not_found"));
    }
    let admitted = api.call("GET", &route, Some(&alice)).send().await;
    assert_eq!(admitted.status, 200);
}

#[tokio::test]
async fn an_svg_named_as_a_png_is_refused_with_its_code_and_writes_no_share() {
    let api = Api::start().await;
    let token = api.token_for("alice").await;
    let svg: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>";
    let reply = upload(&api, &token, form(DOC, &[("img/chart.png", svg)])).await;
    assert_eq!(reply.status, 422, "{}", reply.body);
    assert_eq!(
        reply.code().as_deref(),
        Some("flare::images::format_refused")
    );
    let (status, listed) = api
        .read("GET", "/api/shares?scope=mine", Some(&token))
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        listed["items"].as_array().map(Vec::len),
        Some(0),
        "{listed}"
    );
}

#[tokio::test]
async fn a_form_over_4_mib_is_refused_as_too_large_and_one_under_it_is_read() {
    let api = Api::start().await;
    let token = api.token_for("alice").await;
    let mut padded = png(8);
    padded.resize(4 * 1024 * 1024, 0);
    let reply = upload(&api, &token, form(DOC, &[("img/chart.png", &padded)])).await;
    assert_eq!(reply.status, 413, "{}", reply.body);
    padded.truncate(3 * 1024 * 1024);
    let reply = upload(&api, &token, form(DOC, &[("img/chart.png", &padded)])).await;
    assert_eq!(reply.status, 201, "{}", reply.body);
}

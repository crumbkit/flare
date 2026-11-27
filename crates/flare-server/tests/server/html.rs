//! An uploaded HTML share: the multipart upload, the reading page that
//! frames it, and the content host that serves the file to the frame. Each
//! refusal also shows the admitted case.

use crate::api::Api;
use crate::harness::Reply;
use serde_json::{Value, json};
use std::time::Duration;

const BOUNDARY: &str = "flare-test-html";
const PAGE: &str = "<!doctype html><title>Plan</title><script>document.title = 'ran';</script>\n";

/// A form with an HTML share whose `html` part is `page`.
fn form(page: &[u8]) -> Vec<u8> {
    let share = json!({
        "title": "Plan",
        "format": "html",
        "content": "",
        "with": ["bob@example.com"],
        "re": [],
    });
    let mut body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"share\"\r\nContent-Type: application/json\r\n\r\n{share}\r\n--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"html\"; filename=\"plan.html\"\r\nContent-Type: text/html\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(page);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    body
}

async fn upload(api: &Api, token: &str, body: Vec<u8>) -> Reply {
    api.call("POST", "/api/shares", Some(token))
        .body(&format!("multipart/form-data; boundary={BOUNDARY}"), body)
        .send()
        .await
}

/// The path of the framed view in the reading page `body`.
fn frame_src(body: &str) -> String {
    let start = body.find("src=\"").expect("the page frames the file") + 5;
    let end = start + body[start..].find('"').unwrap();
    body[start..end].replace("&amp;", "&")
}

/// Alice's HTML share for bob: the share's id and the frame's address on
/// the content host.
async fn shared(api: &Api) -> (String, String) {
    let token = api.token_for("alice").await;
    let reply = upload(api, &token, form(PAGE.as_bytes())).await;
    assert_eq!(reply.status, 201, "{}", reply.body);
    let shared: Value = serde_json::from_str(&reply.body).unwrap();
    let id = shared["share"]["id"].as_str().unwrap().to_owned();
    let bob = api.session("bob").await;
    let page = api.browser("GET", &format!("/s/{id}"), &bob).send().await;
    assert_eq!(page.status, 200, "{}", page.body);
    (id, frame_src(&page.body))
}

fn content_path(src: &str) -> String {
    let at = src.find("/v/").expect("the frame names a view token");
    src[at..].to_owned()
}

fn content_host(api: &Api) -> String {
    format!("content.localhost:{}", api.hosted.port)
}

/// A request to the content host as a browser's frame makes it.
fn framed(api: &Api, path: &str) -> crate::harness::Call {
    api.hosted
        .request("GET", path)
        .host(&content_host(api))
        .header("Sec-Fetch-Dest", "iframe")
}

#[tokio::test]
async fn a_reader_sees_the_uploaded_page_in_a_sandboxed_frame_and_the_content_host_serves_it() {
    let api = Api::start_with_content_host().await;
    let (id, src) = shared(&api).await;
    let bob = api.session("bob").await;
    let page = api.browser("GET", &format!("/s/{id}"), &bob).send().await;
    assert!(
        page.body.contains("sandbox=\"allow-scripts\""),
        "{}",
        page.body
    );
    assert!(page.body.contains("referrerpolicy=\"no-referrer\""));
    let policy = page.header("content-security-policy").unwrap();
    assert!(
        policy.contains(&format!(
            "frame-src http://content.localhost:{}",
            api.hosted.port
        )),
        "{policy}"
    );
    let served = framed(&api, &content_path(&src)).send().await;
    assert_eq!(served.status, 200, "{}", served.body);
    assert_eq!(served.body, PAGE);
    assert_eq!(
        served.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(served.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(served.header("cache-control"), Some("private, no-store"));
    assert_eq!(served.header("referrer-policy"), Some("no-referrer"));
    assert_eq!(served.header("x-frame-options"), None);
    assert_eq!(served.header("set-cookie"), None);
    assert_eq!(served.header("access-control-allow-origin"), None);
    let csp = served.header("content-security-policy").unwrap();
    assert!(
        csp.starts_with("sandbox allow-scripts; default-src 'none'"),
        "{csp}"
    );
    assert!(csp.contains("connect-src 'none'"));
    assert!(csp.contains(&format!(
        "frame-ancestors http://localhost:{}",
        api.hosted.port
    )));
}

#[tokio::test]
async fn the_content_host_refuses_an_address_opened_outside_a_frame_with_not_framed() {
    let api = Api::start_with_content_host().await;
    let (_, src) = shared(&api).await;
    let path = content_path(&src);
    let bare = api
        .hosted
        .request("GET", &path)
        .host(&content_host(&api))
        .send()
        .await;
    assert_eq!(bare.status, 403);
    assert_eq!(bare.code().as_deref(), Some("content::not_framed"));
    assert_eq!(framed(&api, &path).send().await.status, 200);
}

#[tokio::test]
async fn a_view_token_expires_after_its_lifetime_with_a_410() {
    let api = Api::start_with_content_host().await;
    let (_, src) = shared(&api).await;
    let path = content_path(&src);
    api.hosted.time.advance(Duration::from_secs(300));
    assert_eq!(framed(&api, &path).send().await.status, 200);
    api.hosted.time.advance(Duration::from_secs(400));
    assert_eq!(framed(&api, &path).send().await.status, 410);
}

#[tokio::test]
async fn a_forged_token_and_an_unknown_path_get_a_plain_404() {
    let api = Api::start_with_content_host().await;
    let (_, src) = shared(&api).await;
    let path = content_path(&src);
    let forged = format!("{}x", path);
    for bad in [forged.as_str(), "/v/1.not-a-token", "/other", "/"] {
        let reply = framed(&api, bad).send().await;
        assert_eq!(reply.status, 404, "{bad}");
    }
    assert_eq!(framed(&api, &path).send().await.status, 200);
}

#[tokio::test]
async fn the_content_host_serves_nothing_but_the_view_route_and_other_hosts_get_403() {
    let api = Api::start_with_content_host().await;
    let (id, _) = shared(&api).await;
    let own = api
        .hosted
        .request("GET", &format!("/s/{id}"))
        .host(&content_host(&api))
        .send()
        .await;
    assert_eq!(own.status, 404);
    let foreign = api
        .hosted
        .request("GET", "/v/x")
        .host("evil.localhost")
        .header("Sec-Fetch-Dest", "iframe")
        .send()
        .await;
    assert_eq!(foreign.status, 403);
    assert_eq!(foreign.code().as_deref(), Some("request::host"));
}

#[tokio::test]
async fn a_token_stops_opening_once_the_viewer_loses_the_share() {
    let api = Api::start_with_content_host().await;
    let (id, src) = shared(&api).await;
    let path = content_path(&src);
    assert_eq!(framed(&api, &path).send().await.status, 200);
    let alice = api.token_for("alice").await;
    let (status, revoked) = api
        .json(
            "DELETE",
            &format!("/api/grants/{id}"),
            Some(&alice),
            json!({"with": ["bob@example.com"]}),
        )
        .await;
    assert_eq!(status, 200, "{revoked}");
    assert_eq!(framed(&api, &path).send().await.status, 404);
}

#[tokio::test]
async fn an_html_upload_without_a_content_host_is_refused_and_stores_nothing() {
    let api = Api::start().await;
    let token = api.token_for("alice").await;
    let reply = upload(&api, &token, form(PAGE.as_bytes())).await;
    assert_eq!(reply.status, 422, "{}", reply.body);
    assert_eq!(
        reply.code().as_deref(),
        Some("flare::html::no_content_host")
    );
    let (_, listed) = api
        .read("GET", "/api/shares?scope=mine", Some(&token))
        .await;
    assert_eq!(listed["items"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn a_file_that_is_not_utf8_or_is_over_4_mib_is_refused_and_one_at_4_mib_is_stored() {
    let api = Api::start_with_content_host().await;
    let token = api.token_for("alice").await;
    let bad = upload(&api, &token, form(&[0xff, 0xfe, b'<'])).await;
    assert_eq!(bad.status, 422, "{}", bad.body);
    assert_eq!(bad.code().as_deref(), Some("flare::html::not_utf8"));
    let big = upload(&api, &token, form(&vec![b'a'; flare_core::MAX_HTML + 1])).await;
    assert_eq!(big.status, 413, "{}", big.body);
    assert_eq!(big.code().as_deref(), Some("flare::html::too_large"));
    let exact = upload(&api, &token, form(&vec![b'a'; flare_core::MAX_HTML])).await;
    assert_eq!(exact.status, 201, "{}", exact.body);
}

#[tokio::test]
async fn html_sent_as_json_is_refused_as_needing_a_form() {
    let api = Api::start_with_content_host().await;
    let token = api.token_for("alice").await;
    let (status, body) = api
        .json(
            "POST",
            "/api/shares",
            Some(&token),
            json!({"title": "Plan", "format": "html", "content": "<p>x</p>", "with": [], "re": []}),
        )
        .await;
    assert_eq!(status, 415, "{body}");
    assert_eq!(body["code"], "flare::html::needs_form");
}

#[tokio::test]
async fn a_new_version_of_an_html_share_replaces_the_file_and_a_markdown_version_is_refused() {
    let api = Api::start_with_content_host().await;
    let (id, _) = shared(&api).await;
    let alice = api.token_for("alice").await;
    let share = json!({"format": "html", "content": ""});
    let mut body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"share\"\r\nContent-Type: application/json\r\n\r\n{share}\r\n--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"html\"; filename=\"plan.html\"\r\nContent-Type: text/html\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(b"<p>second</p>");
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    let second = api
        .call("POST", &format!("/api/shares/{id}/versions"), Some(&alice))
        .body(&format!("multipart/form-data; boundary={BOUNDARY}"), body)
        .send()
        .await;
    assert_eq!(second.status, 201, "{}", second.body);
    let bob = api.session("bob").await;
    let page = api.browser("GET", &format!("/s/{id}"), &bob).send().await;
    let served = framed(&api, &content_path(&frame_src(&page.body)))
        .send()
        .await;
    assert_eq!(served.body, "<p>second</p>");
    let (status, refused) = api
        .json(
            "POST",
            &format!("/api/shares/{id}/versions"),
            Some(&alice),
            json!({"format": "markdown", "content": "# Not html"}),
        )
        .await;
    assert_eq!(status, 422, "{refused}");
    assert_eq!(refused["code"], "flare::versions::kind_changed");
}

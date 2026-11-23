//! Plan tests 5, 6 and 8: no GET route writes, names that try to leave
//! their segment get 400, and pages and bindings render inside a sandboxed
//! `srcdoc` frame under the frame policy.

use crate::harness::{BINDING, PAGE, Server, unescape};
use crate::headers::concrete;
use std::sync::atomic::Ordering;

/// The frame policy that `frame.rs` puts first in the frame's head.
const FRAME_POLICY: &str = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:\">";

/// The decoded `srcdoc` of the one frame in `body`, and the body with the
/// attribute's value removed.
fn frame(body: &str) -> (String, String) {
    let key = "<iframe sandbox srcdoc=\"";
    let start = body
        .find(key)
        .unwrap_or_else(|| panic!("no sandboxed srcdoc frame in {body}"))
        + key.len();
    let end = body[start..].find('"').unwrap() + start;
    assert_eq!(body.matches("<iframe").count(), 1, "one frame");
    let outside = format!("{}{}", &body[..start], &body[end..]);
    (unescape(&body[start..end]), outside)
}

#[tokio::test]
async fn no_get_route_writes_a_page_a_binding_or_a_record() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut gets = 0;
    for (method, pattern) in hangar_server::routes() {
        if method != http::Method::GET {
            continue;
        }
        gets += 1;
        let path = concrete(pattern);
        for query in ["", "?notice=saved", "?code=x&nonce=y&return_to=/flare/"] {
            let reply = server
                .get(&format!("{path}{query}"))
                .cookie(&cookie)
                .send()
                .await;
            assert!(
                reply.status < 500,
                "GET {path}{query} answered {}",
                reply.status
            );
        }
    }
    assert!(gets >= 9, "the table lists only {gets} GET routes");
    assert_eq!(server.switches.page_writes.load(Ordering::SeqCst), 0);
    assert_eq!(server.switches.record_writes.load(Ordering::SeqCst), 0);
}

/// Spellings that try to name something outside one segment, or are not
/// names at all.
const SPELLINGS: [&str; 12] = [
    "..",
    "%2e%2e",
    "..%2f",
    "a%2fb",
    "a/b",
    "../kitchen",
    ".",
    "%00",
    "Kitchen",
    "a%20b",
    "",
    "kitchen%2f..%2f..",
];

#[tokio::test]
async fn every_route_with_a_name_answers_400_to_each_spelling_that_is_not_a_name() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    for spelling in SPELLINGS {
        for (method, prefix) in [("GET", "/pages/"), ("GET", "/b/"), ("POST", "/b/")] {
            let path = format!("{prefix}{spelling}");
            let reply = server
                .request(method, &path)
                .cookie(&cookie)
                .same_origin()
                .form(&[("token".to_owned(), "t".to_owned())])
                .send()
                .await;
            assert_eq!(reply.status, 400, "{method} {path}");
            assert_eq!(
                reply.code().as_deref(),
                Some("request::bad_name"),
                "{method} {path}"
            );
        }
    }
    let admitted = server
        .get(&format!("/b/{BINDING}"))
        .cookie(&cookie)
        .send()
        .await;
    assert_eq!(admitted.status, 200);
}

#[tokio::test]
async fn a_page_renders_from_its_samples_inside_a_sandboxed_srcdoc_frame() {
    let server = Server::start().await;
    server.seed_page().await;
    let cookie = server.sign_in().await;
    let reply = server
        .get(&format!("/pages/{PAGE}"))
        .cookie(&cookie)
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        reply.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    let (document, outside) = frame(&reply.body);
    assert!(
        document.contains("<h1 class=\"card\">A sample task</h1>"),
        "{document}"
    );
    assert!(
        !outside.contains("class=\"card\""),
        "page markup outside the frame: {outside}"
    );
    assert!(outside.contains(PAGE), "the band names the page");
    assert!(outside.contains("<link rel=\"stylesheet\" href=\"/flare/style.css\">"));
}

#[tokio::test]
async fn the_frame_documents_head_starts_with_the_frame_policy_before_any_page_markup() {
    let server = Server::start().await;
    server
        .seed_page_with(PAGE, "<style>h1{color:red}</style><img src=\"https://example.com/x.png\"><h1 class=\"card\">{{task.title}}</h1>", &["title"])
        .await;
    let cookie = server.sign_in().await;
    let reply = server
        .get(&format!("/pages/{PAGE}"))
        .cookie(&cookie)
        .send()
        .await;
    let (document, _) = frame(&reply.body);
    let head = "<head>\n";
    let after_head = document.find(head).unwrap() + head.len();
    assert!(
        document[after_head..].starts_with(FRAME_POLICY),
        "the frame's head starts with {:?}",
        &document[after_head..after_head + 120.min(document.len() - after_head)]
    );
    assert_eq!(document.matches("Content-Security-Policy").count(), 1);
    let policy_at = document.find(FRAME_POLICY).unwrap();
    assert!(policy_at < document.find("<style>").unwrap());
    assert!(policy_at < document.find("<img").unwrap());
}

#[tokio::test]
async fn an_ampersand_and_an_entity_in_page_content_reach_the_frame_unchanged() {
    let server = Server::start().await;
    server
        .seed_page_with(
            PAGE,
            "<p class=\"card\">Fish &amp; chips & peas &lt;b&gt; {{task.title}}</p>",
            &["title"],
        )
        .await;
    let cookie = server.sign_in().await;
    let reply = server
        .get(&format!("/pages/{PAGE}"))
        .cookie(&cookie)
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let (document, _) = frame(&reply.body);
    assert!(
        document.contains("Fish &amp; chips & peas &lt;b&gt;"),
        "the frame document holds the page's own text: {document}"
    );
}

#[tokio::test]
async fn a_binding_renders_from_its_records_with_one_edit_form_under_the_frame() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let reply = server
        .get(&format!("/b/{BINDING}"))
        .cookie(&cookie)
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let (document, outside) = frame(&reply.body);
    assert!(document.contains("Replace the tap washer"), "{document}");
    assert!(!document.contains("A sample task"));
    assert!(!outside.contains("class=\"card\""));
    assert_eq!(outside.matches("<form").count(), 1);
    assert!(outside.find("<iframe").unwrap() < outside.find("<form").unwrap());
    assert!(outside.contains(&format!("action=\"/b/{BINDING}\"")));
}

#[tokio::test]
async fn an_unknown_page_or_binding_answers_404_with_its_code() {
    let server = Server::start().await;
    let cookie = server.sign_in().await;
    let page = server
        .get("/pages/nothing-here")
        .cookie(&cookie)
        .send()
        .await;
    assert_eq!(
        (page.status, page.code().as_deref()),
        (404, Some("pages::not_found"))
    );
    let binding = server.get("/b/nothing-here").cookie(&cookie).send().await;
    assert_eq!(
        (binding.status, binding.code().as_deref()),
        (404, Some("bindings::not_found"))
    );
}

#[tokio::test]
async fn the_index_lists_each_page_and_its_bindings_as_links() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let root = server.get("/").cookie(&cookie).send().await;
    assert_eq!(
        (root.status, root.header("location")),
        (303, Some("/flare/"))
    );
    let index = server.get("/flare/").cookie(&cookie).send().await;
    assert_eq!(index.status, 200);
    assert!(
        index.body.contains(&format!("href=\"/pages/{PAGE}\"")),
        "{}",
        index.body
    );
    assert!(index.body.contains(&format!("href=\"/b/{BINDING}\"")));
}

#[tokio::test]
async fn the_stylesheet_holds_flare_renders_rules_and_the_bands() {
    let server = Server::start().await;
    let reply = server.get("/flare/style.css").send().await;
    assert_eq!(reply.status, 200);
    assert_eq!(
        reply.header("content-type"),
        Some("text/css; charset=utf-8")
    );
    assert!(reply.body.starts_with(hangar_render::stylesheet()));
    assert!(reply.body.contains(".band"));
}

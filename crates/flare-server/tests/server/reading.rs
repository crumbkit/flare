//! The reading page of a Markdown share at `/s/<id>` on the hosted site,
//! and the embedded script files at `/flare/assets/<file>`. Each refusal
//! test also shows the admitted case.

use crate::api::Api;
use crate::harness::Reply;
use crate::headers::POLICY;
use std::sync::atomic::Ordering;

/// Artboard 01's sample share, which the acceptance journey also loads.
const SAMPLE: &str = include_str!("../support/sample.md");
const TITLE: &str = "Weather holds for the island ferry";

/// The policy of a page with code or a figure.
fn script_policy() -> String {
    format!("{POLICY}; script-src 'self'")
}

/// A server where alice shared `markdown` with bob, bob's session and the
/// share's id.
async fn shared(markdown: &str) -> (Api, String, String) {
    let api = Api::start().await;
    let token = api.token_for("alice").await;
    let id = api
        .share_markdown(&token, TITLE, markdown, &["bob@example.com"])
        .await;
    let bob = api.session("bob").await;
    (api, bob, id)
}

async fn read(api: &Api, cookie: &str, path: &str) -> Reply {
    api.hosted.get(path).cookie(cookie).send().await
}

#[tokio::test]
async fn a_granted_reader_sees_the_latest_version_of_a_markdown_share() {
    let (api, bob, id) = shared(SAMPLE).await;
    let reply = read(&api, &bob, &format!("/s/{id}")).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        reply.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    let body = &reply.body;
    assert!(body.contains(&format!("<h1>{TITLE}</h1>")), "{body}");
    assert!(
        body.contains("Shared by <b>alice@example.com</b> with you"),
        "{body}"
    );
    assert!(body.contains("version 1"), "{body}");
    assert!(
        body.contains("<span class=\"brand\">flare</span>"),
        "{body}"
    );
    assert!(body.contains("BO</span></header>"), "{body}");
    assert!(body.contains("<figcaption>Figure 1. What happens to one sailing when a forecast arrives.</figcaption>"), "{body}");
    assert!(body.contains("data-lang=\"rust\""), "{body}");
    for absent in ["Inbox", "Shared by me", "Search", "comments", "responses"] {
        assert!(!body.contains(absent), "{absent}");
    }
}

#[tokio::test]
async fn the_owner_sees_whom_the_share_is_granted_to() {
    let (api, _, id) = shared("Text.\n").await;
    let alice = api.session("alice").await;
    let reply = read(&api, &alice, &format!("/s/{id}")).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(
        reply
            .body
            .contains("Shared by you with <span class=\"mono\">bob@example.com</span>"),
        "{}",
        reply.body
    );
}

#[tokio::test]
async fn a_refused_reader_and_a_missing_share_get_the_same_not_found_page() {
    let (api, bob, id) = shared("Text.\n").await;
    assert_eq!(read(&api, &bob, &format!("/s/{id}")).await.status, 200);
    let carol = api.session("carol").await;
    let refused = read(&api, &carol, &format!("/s/{id}")).await;
    let missing = read(&api, &carol, "/s/00000000000000000000000000000000").await;
    let malformed = read(&api, &carol, "/s/not..an-id").await;
    let no_version = read(&api, &bob, &format!("/s/{id}?version=2")).await;
    let bad_version = read(&api, &bob, &format!("/s/{id}?version=0")).await;
    assert_eq!(refused.status, 404, "{}", refused.body);
    assert!(
        refused.body.contains("shares::not_found"),
        "{}",
        refused.body
    );
    assert!(!refused.body.contains(TITLE));
    for other in [&missing, &malformed, &no_version, &bad_version] {
        assert_eq!(other.status, 404);
        assert_eq!(other.body, refused.body);
    }
}

#[tokio::test]
async fn an_older_version_shows_on_request() {
    let (api, bob, id) = shared("The first version.\n").await;
    api.publish("alice", &id, "The second version.\n").await;
    let latest = read(&api, &bob, &format!("/s/{id}")).await;
    assert!(
        latest.body.contains("The second version."),
        "{}",
        latest.body
    );
    assert!(latest.body.contains("version 2"));
    let older = read(&api, &bob, &format!("/s/{id}?version=1")).await;
    assert_eq!(older.status, 200, "{}", older.body);
    assert!(older.body.contains("The first version."), "{}", older.body);
    assert!(older.body.contains("version 1"));
    assert!(!older.body.contains("The second version."));
}

#[tokio::test]
async fn a_browser_with_no_session_is_asked_to_sign_in() {
    let (api, bob, id) = shared("Text.\n").await;
    let reply = api.hosted.get(&format!("/s/{id}")).send().await;
    assert_eq!(reply.status, 401, "{}", reply.body);
    assert!(!reply.body.contains("Text."));
    assert_eq!(read(&api, &bob, &format!("/s/{id}")).await.status, 200);
}

#[tokio::test]
async fn an_unavailable_store_gives_a_503_page_with_retry() {
    let (api, bob, id) = shared("Text.\n").await;
    api.faults.shares.store(true, Ordering::SeqCst);
    let reply = read(&api, &bob, &format!("/s/{id}?version=1")).await;
    assert_eq!(reply.status, 503, "{}", reply.body);
    assert!(reply.body.contains("shares::unavailable"), "{}", reply.body);
    assert!(
        reply
            .body
            .contains(&format!("<a href=\"/s/{id}?version=1\">Retry</a>")),
        "{}",
        reply.body
    );
    api.faults.shares.store(false, Ordering::SeqCst);
    assert_eq!(read(&api, &bob, &format!("/s/{id}")).await.status, 200);
}

#[tokio::test]
async fn only_a_page_with_code_or_a_figure_lets_scripts_run_and_no_policy_allows_eval() {
    let api = Api::start().await;
    let token = api.token_for("alice").await;
    let bob = api.session("bob").await;
    let mut expected = Vec::new();
    for (markdown, scripts) in [
        ("```rust\nfn x() {}\n```\n", true),
        ("```mermaid\npie\n  \"a\" : 1\n```\n", true),
        ("Prose with `inline code`.\n\n| a |\n|---|\n| 1 |\n", false),
        ("```mermaid\njourney\n  title x\n```\n", true),
    ] {
        let id = api
            .share_markdown(&token, TITLE, markdown, &["bob@example.com"])
            .await;
        expected.push((id, scripts));
    }
    for (id, scripts) in expected {
        let reply = read(&api, &bob, &format!("/s/{id}")).await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        let policy = if scripts {
            script_policy()
        } else {
            POLICY.to_owned()
        };
        assert_eq!(reply.all("content-security-policy"), vec![policy.as_str()]);
        assert_eq!(reply.all("cache-control"), vec!["no-store"]);
        assert!(!reply.all("content-security-policy")[0].contains("unsafe-eval"));
        if !scripts {
            assert!(!reply.body.contains("<script"), "{}", reply.body);
        }
    }
}

#[tokio::test]
async fn the_entry_script_is_served_with_an_immutable_cache_and_an_unknown_file_is_not_found() {
    let (api, bob, id) = shared("```rust\nfn x() {}\n```\n").await;
    let page = read(&api, &bob, &format!("/s/{id}")).await;
    let missing = api
        .hosted
        .get("/flare/assets/reading-00000000.js")
        .send()
        .await;
    assert_eq!(missing.status, 404);
    assert_eq!(missing.all("cache-control"), vec!["no-store"]);
    let Some(start) = page.body.find("src=\"/flare/assets/") else {
        // A build with no bundle loads no script; the reading job in CI
        // builds the bundle and requires it.
        return;
    };
    let path = page.body[start + 5..].split('"').next().unwrap().to_owned();
    let script = api.hosted.get(&path).send().await;
    assert_eq!(script.status, 200, "{path}");
    assert_eq!(
        script.header("content-type"),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(
        script.all("cache-control"),
        vec!["public, max-age=31536000, immutable"]
    );
}

/// Writes artboard 01's sample share, as a reader sees it, for the
/// acceptance journey in ui/flare: the page, its policy and the stylesheet,
/// into the directory that `FLARE_READING_SAMPLE` names. With the variable
/// unset the test only checks the page.
#[tokio::test]
async fn the_sample_share_renders_for_the_acceptance_journey() {
    let (api, bob, id) = shared(SAMPLE).await;
    let page = read(&api, &bob, &format!("/s/{id}")).await;
    assert_eq!(page.status, 200, "{}", page.body);
    let style = api.hosted.get("/flare/style.css").send().await;
    assert_eq!(style.status, 200);
    #[expect(
        clippy::disallowed_methods,
        reason = "the acceptance journey names the directory for this test's output"
    )]
    let Some(directory) = std::env::var_os("FLARE_READING_SAMPLE") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let policy = page.all("content-security-policy").join("");
    for (name, text) in [
        ("page.html", page.body.as_str()),
        ("policy.txt", policy.as_str()),
        ("style.css", style.body.as_str()),
    ] {
        #[expect(
            clippy::disallowed_methods,
            reason = "the test writes fixture files for the acceptance journey, which nothing else reads"
        )]
        std::fs::write(directory.join(name), text).unwrap();
    }
}

#[tokio::test]
async fn the_reading_pages_fonts_resolve_on_flares_own_origin() {
    let (api, bob, id) = shared(SAMPLE).await;
    let page = read(&api, &bob, &format!("/s/{id}")).await;
    assert!(
        page.body.contains("href=\"/flare/style.css\""),
        "{}",
        page.body
    );
    let style = api.hosted.get("/flare/style.css").send().await;
    let urls: Vec<&str> = style
        .body
        .split("url(")
        .skip(1)
        .filter_map(|rest| rest.split(')').next())
        .map(|url| url.trim_matches('"'))
        .collect();
    let fonts: Vec<&&str> = urls.iter().filter(|url| url.ends_with(".woff2")).collect();
    assert_eq!(fonts.len(), 2, "{urls:?}");
    for url in &urls {
        assert!(
            url.starts_with("/flare/assets/"),
            "{url} is not on flare's origin"
        );
    }
    for url in fonts {
        let font = api.hosted.get(url).send().await;
        assert_eq!(font.status, 200, "{url}");
        assert_eq!(font.header("content-type"), Some("font/woff2"), "{url}");
        assert_eq!(
            font.all("cache-control"),
            vec!["public, max-age=31536000, immutable"],
            "{url}"
        );
    }
    assert!(!style.body.contains("fonts.googleapis.com"));
    assert!(!style.body.contains("fonts.gstatic.com"));
}

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

/// flare's policy on the hosted site, which adds `connect-src 'self'`.
fn hosted_policy() -> String {
    POLICY.replace("font-src 'self';", "font-src 'self'; connect-src 'self';")
}

/// The policy of a page that loads a script: the hosted policy with the
/// page's own scripts allowed. Its `connect-src 'self'` lets the comments
/// section call the comment routes.
fn script_policy() -> String {
    format!("{}; script-src 'self'", hosted_policy())
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

/// The get flare address of the tests' badged site.
const GET_FLARE: &str = "https://example.com/flare";

/// The link the foot holds for a reader who has never shared.
const BADGE: &str = "<a class=\"badge\" href=\"https://example.com/flare\" target=\"_blank\" rel=\"noopener noreferrer\" aria-label=\"Get Flare, opens in a new tab\">";

/// A site that links to [`GET_FLARE`], where alice shared `markdown` with
/// bob, bob's session and the share's id.
async fn badged(markdown: &str) -> (Api, String, String) {
    let api = Api::with_get_flare(GET_FLARE).await;
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
    // The drawing's box is named by the diagram's accDescr, the mockup's
    // description of its branches, so a screen reader does not hear the
    // caption twice.
    assert!(
        body.contains("<div class=\"dg\" role=\"img\" aria-label=\"Flowchart. If the forecast gust is not over the limit, the sailing stays on. If it is, the sailing is held for the duty master&#39;s review. If the duty master confirms within 40 minutes, the sailing is cancelled and passengers are rebooked. If not, the sailing stays on.\""),
        "{body}"
    );
    assert!(body.contains("data-lang=\"rust\""), "{body}");
    for absent in ["Inbox", "Shared by me", "Search", "responses"] {
        assert!(!body.contains(absent), "{absent}");
    }
    assert!(
        body.contains("<a class=\"chip\" href=\"#comments\">No comments</a>"),
        "{body}"
    );
    assert!(
        body.contains(&format!("<section id=\"comments\" class=\"comments\" aria-labelledby=\"comments-heading\" data-flare=\"{id}\" data-version=\"1\" data-owner=\"alice@example.com\">")),
        "{body}"
    );
}

/// Posts `body` on `flare` as the holder of `token`, as a reply to
/// `parent` when it is given, and returns the post's id.
async fn post(api: &Api, token: &str, flare: &str, parent: Option<&str>, key: &str) -> String {
    let mut body = serde_json::json!({"flare": flare, "body": "Text.", "key": key});
    if let Some(parent) = parent {
        body["parent"] = serde_json::json!(parent);
    }
    let (status, posted) = api.json("POST", "/api/comments", Some(token), body).await;
    assert_eq!(status, 201, "{posted}");
    posted["id"].as_str().unwrap().to_owned()
}

/// The text of the comment count under the title.
fn count(page: &Reply) -> &str {
    page.body
        .split_once("<a class=\"chip\" href=\"#comments\">")
        .and_then(|(_, rest)| rest.split_once("</a>"))
        .map(|(text, _)| text)
        .unwrap()
}

#[tokio::test]
async fn the_line_under_the_title_counts_comments_and_replies_as_the_section_heading_does() {
    let (api, bob, id) = shared("Text.\n").await;
    let token = api.token_for("bob").await;
    let first = post(&api, &token, &id, None, "one").await;
    assert_eq!(
        count(&read(&api, &bob, &format!("/s/{id}")).await),
        "1 comment"
    );
    let reply = post(&api, &token, &id, Some(&first), "two").await;
    let second = post(&api, &token, &id, None, "three").await;
    assert_eq!(
        count(&read(&api, &bob, &format!("/s/{id}")).await),
        "2 comments, 1 reply"
    );
    // A deleted comment that still has a reply keeps its place in the
    // section, so it still counts; a deleted comment with none does not.
    for deleted in [&first, &second] {
        let (status, body) = api
            .read("DELETE", &format!("/api/comments/{deleted}"), Some(&token))
            .await;
        assert!(status < 300, "{status} {body}");
    }
    assert_eq!(
        count(&read(&api, &bob, &format!("/s/{id}")).await),
        "1 comment, 1 reply"
    );
    let (status, body) = api
        .read("DELETE", &format!("/api/comments/{reply}"), Some(&token))
        .await;
    assert!(status < 300, "{status} {body}");
    assert_eq!(
        count(&read(&api, &bob, &format!("/s/{id}")).await),
        "No comments"
    );
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
async fn a_page_that_loads_a_script_may_call_its_own_origin_and_no_policy_allows_eval() {
    let api = Api::start().await;
    let token = api.token_for("alice").await;
    let bob = api.session("bob").await;
    let mut ids = Vec::new();
    for markdown in [
        "```rust\nfn x() {}\n```\n",
        "```mermaid\npie\n  \"a\" : 1\n```\n",
        "Prose with `inline code`.\n\n| a |\n|---|\n| 1 |\n",
        "```mermaid\njourney\n  title x\n```\n",
    ] {
        ids.push(
            api.share_markdown(&token, TITLE, markdown, &["bob@example.com"])
                .await,
        );
    }
    for id in ids {
        let reply = read(&api, &bob, &format!("/s/{id}")).await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        // A build with no bundle loads no script; the reading job in CI
        // builds the bundle and requires it, so every page there loads the
        // comments entry.
        let policy = if reply.body.contains("<script") {
            script_policy()
        } else {
            hosted_policy()
        };
        assert_eq!(reply.all("content-security-policy"), vec![policy.as_str()]);
        assert_eq!(reply.all("cache-control"), vec!["no-store"]);
        assert!(!policy.contains("unsafe-eval"));
        assert!(policy.contains("connect-src 'self'"));
    }
}

#[tokio::test]
async fn a_page_without_code_loads_only_the_comments_entry() {
    let (api, bob, id) = shared("Prose and a table.\n\n| a |\n|---|\n| 1 |\n").await;
    let page = read(&api, &bob, &format!("/s/{id}")).await;
    let scripts: Vec<&str> = page
        .body
        .split("<script type=\"module\" src=\"/flare/assets/")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect();
    // A build with no bundle loads no script.
    if !scripts.is_empty() {
        assert_eq!(scripts.len(), 1, "{scripts:?}");
        assert!(scripts[0].starts_with("comments-"), "{scripts:?}");
    }
}

#[tokio::test]
async fn a_page_whose_only_blocks_are_diagrams_shows_them_drawn_and_loads_only_the_comments_entry()
{
    let (api, bob, id) = shared(
        "```mermaid\nflowchart LR\n  a[Forecast] --> b{Held?}\n```\n\nFigure 1. A forecast.\n",
    )
    .await;
    let page = read(&api, &bob, &format!("/s/{id}")).await;
    assert_eq!(page.status, 200, "{}", page.body);
    assert!(
        page.body
            .contains("<figure class=\"diagram\" data-diagram=\"drawn\">"),
        "{}",
        page.body
    );
    assert!(page.body.contains("<svg "), "{}", page.body);
    assert!(!page.body.contains("diagram-source"), "{}", page.body);
    let scripts: Vec<&str> = page
        .body
        .split("<script type=\"module\" src=\"/flare/assets/")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect();
    // A build with no bundle loads no script.
    if !scripts.is_empty() {
        assert_eq!(scripts.len(), 1, "{scripts:?}");
        assert!(scripts[0].starts_with("comments-"), "{scripts:?}");
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
    let (api, bob, id) = badged(SAMPLE).await;
    let page = read(&api, &bob, &format!("/s/{id}")).await;
    assert_eq!(page.status, 200, "{}", page.body);
    assert!(page.body.contains(BADGE), "{}", page.body);
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

#[tokio::test]
async fn a_reader_who_has_never_shared_sees_the_get_flare_link_at_the_foot() {
    let (api, bob, id) = badged("Text.\n").await;
    let reply = read(&api, &bob, &format!("/s/{id}")).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let body = &reply.body;
    assert!(body.contains("<footer class=\"share-foot\">"), "{body}");
    assert!(body.contains("Shared with Flare"), "{body}");
    assert!(body.contains(BADGE), "{body}");
    assert!(body.find("</article>") < body.find(BADGE), "{body}");
}

#[tokio::test]
async fn a_reader_who_has_shared_sees_the_note_and_no_link() {
    let (api, bob, id) = badged("Text.\n").await;
    assert!(
        read(&api, &bob, &format!("/s/{id}"))
            .await
            .body
            .contains(BADGE)
    );
    let token = api.token_for("bob").await;
    api.share(&token, "Bob's plan", &[], &[]).await;
    let reply = read(&api, &bob, &format!("/s/{id}")).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(reply.body.contains("Shared with Flare"), "{}", reply.body);
    assert!(!reply.body.contains("Get Flare"), "{}", reply.body);
}

#[tokio::test]
async fn the_owner_reading_their_own_share_sees_no_link() {
    let (api, _, id) = badged("Text.\n").await;
    let alice = api.session("alice").await;
    let reply = read(&api, &alice, &format!("/s/{id}")).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(reply.body.contains("Shared with Flare"), "{}", reply.body);
    assert!(!reply.body.contains("Get Flare"), "{}", reply.body);
}

#[tokio::test]
async fn a_site_with_no_get_flare_address_shows_every_reader_the_note_and_no_link() {
    let (api, bob, id) = shared("Text.\n").await;
    let reply = read(&api, &bob, &format!("/s/{id}")).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(reply.body.contains("Shared with Flare"), "{}", reply.body);
    assert!(!reply.body.contains("Get Flare"), "{}", reply.body);
}

/// The comments journey of `ui/flare/tests/acceptance/comments.spec.ts`,
/// run in Chromium against this server and its memory stores. alice shares
/// artboard 01's sample with bob, and bob's session cookie, which he got by
/// signing in through the fake provider, stands for his sign-in in the
/// browser. The journey posts a comment and a reply, reloads and sees both.
/// `FLARE_COMMENTS_JOURNEY` names the `ui/flare` directory; with it unset
/// the test only checks that the page holds the section the journey drives.
#[tokio::test(flavor = "multi_thread")]
async fn a_reader_comments_and_replies_in_a_browser_and_sees_both_after_a_reload() {
    let (api, bob, id) = shared(SAMPLE).await;
    let page = read(&api, &bob, &format!("/s/{id}")).await;
    assert!(
        page.body.contains("<section id=\"comments\""),
        "{}",
        page.body
    );
    #[expect(
        clippy::disallowed_methods,
        reason = "CI names the ui/flare directory for the journey this test starts"
    )]
    let Some(ui) = std::env::var_os("FLARE_COMMENTS_JOURNEY") else {
        return;
    };
    let origin = api.hosted.origin.clone();
    let status = tokio::task::block_in_place(|| {
        #[expect(
            clippy::disallowed_methods,
            reason = "the test runs Playwright, which drives a browser against this server, and waits for it to exit"
        )]
        std::process::Command::new("pnpm")
            .args(["exec", "playwright", "test", "tests/acceptance/comments.spec.ts"])
            .current_dir(ui)
            .env("FLARE_JOURNEY_ORIGIN", &origin)
            .env("FLARE_JOURNEY_COOKIE", &bob)
            .env("FLARE_JOURNEY_SHARE", &id)
            .status()
    })
    .unwrap();
    assert!(status.success(), "the comments journey failed: {status}");
}

//! Plan test 10: what each outcome of an edit answers.

use crate::forms::{draw, post};
use crate::harness::{BINDING, BODY, RECORD, Server, form_pairs, set, value};
use std::sync::atomic::Ordering;

#[tokio::test]
async fn a_post_that_changes_nothing_writes_nothing_and_says_so() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let pairs = draw(&server, BINDING, &cookie).await;
    let reply = post(&server, BINDING, &cookie, &pairs).await;
    assert_eq!(reply.status, 303, "{}", reply.body);
    assert_eq!(
        reply.header("location"),
        Some("/b/kitchen?notice=unchanged")
    );
    assert_eq!(server.switches.record_writes.load(Ordering::SeqCst), 0);
    assert_eq!(server.body_of(RECORD).await, BODY);
}

#[tokio::test]
async fn the_notice_query_selects_only_saved_or_unchanged() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let page = |query: &'static str| {
        let server = &server;
        let cookie = cookie.clone();
        async move {
            server
                .get(&format!("/b/{BINDING}{query}"))
                .cookie(&cookie)
                .send()
                .await
                .body
        }
    };
    let fresh = page("").await;
    let saved = page("?notice=saved").await;
    let unchanged = page("?notice=unchanged").await;
    let other = page("?notice=%3Cscript%3E").await;
    assert!(saved.contains("<b>Saved.</b>"), "{saved}");
    assert!(unchanged.contains("<b>Nothing to save.</b>"));
    for body in [&fresh, &other] {
        assert_eq!(body.matches("class=\"msg").count(), 0, "{body}");
    }
    assert!(!other.contains("<script>"));
}

#[tokio::test]
async fn a_write_the_store_rejects_draws_the_form_again_with_the_entries_and_the_reason() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &cookie).await;
    set(&mut pairs, "f.title", "Kept <entry>");
    *server.switches.reject.lock().unwrap() = Some("the title breaks a <rule>".to_owned());
    let reply = post(&server, BINDING, &cookie, &pairs).await;
    assert_eq!(reply.status, 422, "{}", reply.body);
    assert_eq!(reply.code().as_deref(), Some("forms::refused"));
    assert!(
        reply.body.contains("the title breaks a &lt;rule&gt;"),
        "{}",
        reply.body
    );
    let again = form_pairs(&reply.body);
    assert_eq!(value(&again, "f.title"), "Kept <entry>");
    assert_ne!(
        value(&again, "token"),
        value(&pairs, "token"),
        "a new token"
    );
    assert_eq!(server.body_of(RECORD).await, BODY);
    assert_eq!(post(&server, BINDING, &cookie, &again).await.status, 303);
    assert!(server.body_of(RECORD).await.contains("Kept <entry>"));
}

#[tokio::test]
async fn an_entry_that_does_not_fit_the_schema_draws_the_form_again() {
    let server = Server::start().await;
    let version = server
        .seed_page_with("status-card", "<p>{{task.status}}</p>", &["status"])
        .await;
    server.seed_record(RECORD, BODY);
    server
        .save_binding(BINDING, "status-card", &version, RECORD)
        .await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &cookie).await;
    pairs.retain(|(key, _)| key != "f.status");
    pairs.push(("f.status".to_owned(), "lost".to_owned()));
    let reply = post(&server, BINDING, &cookie, &pairs).await;
    assert_eq!(reply.status, 422, "{}", reply.body);
    assert_eq!(server.body_of(RECORD).await, BODY);
}

#[tokio::test]
async fn an_edit_from_a_stale_form_shows_the_conflict_view_and_its_form_saves_onto_the_newer_record()
 {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &cookie).await;
    set(&mut pairs, "f.title", "My edit");
    let newer = server.seed_record(
        RECORD,
        r#"{"title": "Changed elsewhere", "status": "open"}"#,
    );
    let reply = post(&server, BINDING, &cookie, &pairs).await;
    assert_eq!(reply.status, 409, "{}", reply.body);
    assert_eq!(reply.code().as_deref(), Some("forms::conflict"));
    assert!(reply.body.contains("Changed elsewhere"));
    assert!(reply.body.contains("class=\"conflict"));
    let merged = form_pairs(&reply.body);
    assert_eq!(value(&merged, "based_on"), newer.as_str());
    assert_ne!(value(&merged, "token"), value(&pairs, "token"));
    assert_eq!(server.switches.record_writes.load(Ordering::SeqCst), 0);
    let saved = post(&server, BINDING, &cookie, &merged).await;
    assert_eq!(saved.status, 303, "{}", saved.body);
    assert!(server.body_of(RECORD).await.contains("My edit"));
}

#[tokio::test]
async fn a_conflict_the_store_finds_is_read_and_applied_again() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &cookie).await;
    set(&mut pairs, "f.title", "Second try");
    server.switches.conflicts.store(2, Ordering::SeqCst);
    let reply = post(&server, BINDING, &cookie, &pairs).await;
    assert_eq!(reply.status, 303, "{}", reply.body);
    assert_eq!(server.switches.record_writes.load(Ordering::SeqCst), 1);
    assert!(server.body_of(RECORD).await.contains("Second try"));
}

#[tokio::test]
async fn a_store_that_keeps_finding_conflicts_answers_503_after_three_tries() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &cookie).await;
    set(&mut pairs, "f.title", "Never lands");
    server.switches.conflicts.store(4, Ordering::SeqCst);
    let reply = post(&server, BINDING, &cookie, &pairs).await;
    assert_eq!(reply.status, 503, "{}", reply.body);
    assert_eq!(reply.code().as_deref(), Some("records::busy"));
    assert_eq!(
        server.switches.conflicts.load(Ordering::SeqCst),
        1,
        "exactly three tries"
    );
    assert_eq!(server.body_of(RECORD).await, BODY);
}

#[tokio::test]
async fn a_post_of_the_wrong_content_type_or_over_one_mebibyte_writes_nothing() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let call = server
        .request("POST", &format!("/b/{BINDING}"))
        .cookie(&cookie)
        .same_origin();
    let json = call
        .clone()
        .body("application/json", b"{}".to_vec())
        .send()
        .await;
    assert_eq!(
        (json.status, json.code().as_deref()),
        (415, Some("request::unsupported_type"))
    );
    let big = call
        .body(
            "application/x-www-form-urlencoded",
            vec![b'a'; 1024 * 1024 + 1],
        )
        .send()
        .await;
    assert_eq!(
        (big.status, big.code().as_deref()),
        (413, Some("request::body_too_large"))
    );
    assert_eq!(server.body_of(RECORD).await, BODY);
}

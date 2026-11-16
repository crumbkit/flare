//! Plan test 9: the form token. A good post writes once, and each post
//! whose token, session, binding, slot, blob or fields differ from what the
//! token was issued for is refused with the record unchanged.

use crate::harness::{
    BINDING, BODY, Options, PAGE, RECORD, Reply, Server, form_pairs, limits, set,
};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Draws the edit form of `binding` for `cookie` and returns its pairs.
pub(crate) async fn draw(server: &Server, binding: &str, cookie: &str) -> Vec<(String, String)> {
    let reply = server
        .get(&format!("/b/{binding}"))
        .cookie(cookie)
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    form_pairs(&reply.body)
}

/// Posts `pairs` to `binding` as a browser on the launch origin does.
pub(crate) async fn post(
    server: &Server,
    binding: &str,
    cookie: &str,
    pairs: &[(String, String)],
) -> Reply {
    server
        .request("POST", &format!("/b/{binding}"))
        .cookie(cookie)
        .same_origin()
        .form(pairs)
        .send()
        .await
}

fn writes(server: &Server) -> usize {
    server.switches.record_writes.load(Ordering::SeqCst)
}

fn assert_refused(reply: &Reply, status: u16, code: &str) {
    assert_eq!(reply.status, status, "{}", reply.body);
    assert_eq!(reply.code().as_deref(), Some(code), "{}", reply.body);
}

#[tokio::test]
async fn a_good_post_writes_the_record_once_and_redirects_to_the_binding() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &cookie).await;
    set(&mut pairs, "f.title", "Fit the new washer");
    let reply = post(&server, BINDING, &cookie, &pairs).await;
    assert_eq!(reply.status, 303, "{}", reply.body);
    assert_eq!(reply.header("location"), Some("/b/kitchen?notice=saved"));
    assert_eq!(writes(&server), 1);
    assert!(server.body_of(RECORD).await.contains("Fit the new washer"));
    let replay = post(&server, BINDING, &cookie, &pairs).await;
    assert_refused(&replay, 403, "forms::token_unknown");
    assert_eq!(writes(&server), 1, "a replayed token writes nothing");
}

#[tokio::test]
async fn a_token_drawn_for_another_session_is_refused() {
    let server = Server::start().await;
    server.seed_binding().await;
    let drew = server.sign_in().await;
    let other = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &drew).await;
    set(&mut pairs, "f.title", "Edited by another session");
    assert_refused(
        &post(&server, BINDING, &other, &pairs).await,
        403,
        "forms::token_mismatch",
    );
    assert_refused(
        &post(&server, BINDING, &drew, &pairs).await,
        403,
        "forms::token_unknown",
    );
    assert_eq!(server.body_of(RECORD).await, BODY);
}

#[tokio::test]
async fn a_token_drawn_for_another_binding_is_refused_on_this_one() {
    let server = Server::start().await;
    let version = server.seed_page().await;
    server.seed_record(RECORD, BODY);
    server.seed_record("/tasks/shelf", BODY);
    server.save_binding(BINDING, PAGE, &version, RECORD).await;
    server
        .save_binding("pantry", PAGE, &version, "/tasks/shelf")
        .await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, "pantry", &cookie).await;
    set(&mut pairs, "f.title", "Wrong binding");
    set(&mut pairs, "through", BINDING);
    assert_refused(
        &post(&server, BINDING, &cookie, &pairs).await,
        403,
        "forms::token_mismatch",
    );
    assert_eq!(server.body_of(RECORD).await, BODY);
    assert_eq!(server.body_of("/tasks/shelf").await, BODY);
}

#[tokio::test]
async fn a_post_that_changes_the_slot_the_blob_or_the_binding_it_names_is_refused() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    for (key, value) in [
        ("slot", "other"),
        ("based_on", "0123456789012345678901234567890123456789"),
        ("through", "pantry"),
    ] {
        let mut pairs = draw(&server, BINDING, &cookie).await;
        set(&mut pairs, "f.title", "Changed claim");
        set(&mut pairs, key, value);
        assert_refused(
            &post(&server, BINDING, &cookie, &pairs).await,
            403,
            "forms::token_mismatch",
        );
    }
    assert_eq!(writes(&server), 0);
    assert_eq!(server.body_of(RECORD).await, BODY);
}

#[tokio::test]
async fn a_post_with_an_added_path_field_is_refused_before_any_token_is_trusted() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &cookie).await;
    set(&mut pairs, "f.title", "Elsewhere");
    pairs.push(("path".to_owned(), "/tasks/other".to_owned()));
    let reply = post(&server, BINDING, &cookie, &pairs).await;
    assert_refused(&reply, 400, "forms::bad_post");
    assert!(reply.body.contains(&format!("href=\"/b/{BINDING}\"")));
    assert_eq!(writes(&server), 0);
}

#[tokio::test]
async fn a_post_with_a_bearer_token_and_no_session_is_refused() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &cookie).await;
    set(&mut pairs, "f.title", "From a script");
    let reply = server
        .request("POST", &format!("/b/{BINDING}"))
        .bearer(&server.token())
        .same_origin()
        .form(&pairs)
        .send()
        .await;
    assert_refused(&reply, 403, "forms::needs_session");
    assert_eq!(writes(&server), 0);
}

#[tokio::test]
async fn a_post_without_sec_fetch_site_is_refused() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &cookie).await;
    set(&mut pairs, "f.title", "No fetch metadata");
    let origin = format!("http://{}", server.authority);
    let reply = server
        .request("POST", &format!("/b/{BINDING}"))
        .cookie(&cookie)
        .header("Origin", &origin)
        .form(&pairs)
        .send()
        .await;
    assert_refused(&reply, 403, "request::origin");
    assert_eq!(writes(&server), 0);
    assert_eq!(post(&server, BINDING, &cookie, &pairs).await.status, 303);
}

#[tokio::test]
async fn a_binding_whose_fill_changed_after_the_form_was_drawn_answers_409() {
    let server = Server::start().await;
    let version = server.seed_page().await;
    server.seed_record(RECORD, BODY);
    server.seed_record("/tasks/shelf", BODY);
    server.save_binding(BINDING, PAGE, &version, RECORD).await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &cookie).await;
    set(&mut pairs, "f.title", "Too late");
    server.swap_fill(BINDING, &version, "/tasks/shelf");
    assert_refused(
        &post(&server, BINDING, &cookie, &pairs).await,
        409,
        "forms::binding_changed",
    );
    assert_eq!(server.body_of(RECORD).await, BODY);
    assert_eq!(server.body_of("/tasks/shelf").await, BODY);
}

#[tokio::test]
async fn a_full_token_table_drops_its_oldest_token() {
    let server = Server::start_with(Options {
        limits: Some(limits(|limits| limits.form_tokens = 2)),
        ..Options::default()
    })
    .await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut oldest = draw(&server, BINDING, &cookie).await;
    let _middle = draw(&server, BINDING, &cookie).await;
    let mut newest = draw(&server, BINDING, &cookie).await;
    set(&mut oldest, "f.title", "Oldest");
    set(&mut newest, "f.title", "Newest");
    assert_refused(
        &post(&server, BINDING, &cookie, &oldest).await,
        403,
        "forms::token_unknown",
    );
    assert_eq!(post(&server, BINDING, &cookie, &newest).await.status, 303);
}

#[tokio::test]
async fn a_token_lives_one_hour() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut late = draw(&server, BINDING, &cookie).await;
    server.time.advance(Duration::from_secs(59 * 60));
    let mut in_time = draw(&server, BINDING, &cookie).await;
    server.time.advance(Duration::from_secs(60 + 1));
    set(&mut late, "f.title", "Late");
    set(&mut in_time, "f.title", "In time");
    assert_refused(
        &post(&server, BINDING, &cookie, &late).await,
        403,
        "forms::token_unknown",
    );
    assert_eq!(post(&server, BINDING, &cookie, &in_time).await.status, 303);
}

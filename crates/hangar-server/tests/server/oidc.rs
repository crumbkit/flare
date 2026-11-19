//! Plan test 12: the OpenID Connect sign-in through the fake provider, and
//! the hand-over from `localhost` to the launch host.

use crate::fake_provider::{Fake, HOME, TokenReply, connect, connect_with_deadline};
use crate::harness::{Options, Reply, Server, cookie_pair, limits};
use hangar_auth_oidc::TestClock;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use url::Url;

/// A server with OpenID Connect against a new fake, and the page the
/// fake's allow-list reaches.
pub(crate) async fn oidc_server() -> (Server, Fake, TestClock) {
    let fake = Fake::start().await;
    let (authenticator, clock) = connect(&fake).await;
    let server = Server::start_with(Options {
        oidc: Some(authenticator),
        ..Options::default()
    })
    .await;
    server
        .seed_page_with(HOME, "<p>{{task.title}}</p>", &[])
        .await;
    (server, fake, clock)
}

/// Starts a sign-in on the sign-in host: the provider URL and the
/// kept-state cookie pair.
pub(crate) async fn start(server: &Server) -> (Reply, Url, String) {
    let reply = server
        .oidc_request("GET", &format!("/auth/oidc/start?return_to=/pages/{HOME}"))
        .send()
        .await;
    assert_eq!(reply.status, 303, "{}", reply.body);
    let location = Url::parse(reply.header("location").unwrap()).unwrap();
    let kept = cookie_pair(&reply, "hangar_oidc_state").expect("a kept-state cookie");
    (reply, location, kept)
}

/// Sends the provider's answer to the callback.
pub(crate) async fn callback(server: &Server, query: &str, kept: &str) -> Reply {
    server
        .oidc_request("GET", &format!("/auth/oidc/callback?{query}"))
        .cookie(kept)
        .send()
        .await
}

/// The path and query of the launch host's finish route that `reply`
/// redirects to.
pub(crate) fn finish_path(server: &Server, reply: &Reply) -> String {
    let location = Url::parse(reply.header("location").unwrap()).unwrap();
    assert_eq!(
        location
            .host_str()
            .map(|host| format!("{host}:{}", location.port().unwrap())),
        Some(server.authority.clone())
    );
    assert_eq!(location.path(), "/auth/oidc/finish");
    format!("/auth/oidc/finish?{}", location.query().unwrap())
}

/// Signs a browser in through the fake and returns its session cookie
/// pair on the launch host.
pub(crate) async fn sign_in(server: &Server, fake: &Fake) -> String {
    let (_, location, kept) = start(server).await;
    let reply = callback(server, &fake.approve(&location), &kept).await;
    assert_eq!(reply.status, 303, "{}", reply.body);
    let finish = server.get(&finish_path(server, &reply)).send().await;
    assert_eq!(finish.status, 200, "{}", finish.body);
    cookie_pair(&finish, "hangar_session").expect("a session cookie")
}

#[tokio::test]
async fn start_sets_the_kept_state_cookie_on_localhost_and_redirects_to_the_provider() {
    let (server, fake, _clock) = oidc_server().await;
    let (reply, location, _) = start(&server).await;
    assert!(location.as_str().starts_with(fake.issuer()));
    let cookies = reply.all("set-cookie");
    assert_eq!(cookies.len(), 1, "{cookies:?}");
    let cookie = cookies[0];
    for part in ["HttpOnly", "SameSite=Lax", "Path=/auth/oidc", "Max-Age=600"] {
        assert!(
            cookie.split("; ").any(|piece| piece == part),
            "{part} in {cookie}"
        );
    }
    assert!(!cookie.contains("Domain"));
}

#[tokio::test]
async fn the_callback_sets_no_session_cookie_and_clears_the_kept_state() {
    let (server, fake, _clock) = oidc_server().await;
    let (_, location, kept) = start(&server).await;
    let reply = callback(&server, &fake.approve(&location), &kept).await;
    assert_eq!(reply.status, 303, "{}", reply.body);
    assert!(cookie_pair(&reply, "hangar_session").is_none());
    let cleared = reply
        .all("set-cookie")
        .into_iter()
        .find(|cookie| cookie.starts_with("hangar_oidc_state="))
        .expect("the kept state is cleared");
    assert!(cleared.contains("Max-Age=0"));
    finish_path(&server, &reply);
}

#[tokio::test]
async fn the_finish_route_sets_the_session_cookie_on_the_launch_host_and_the_session_reads_its_page()
 {
    let (server, fake, _clock) = oidc_server().await;
    let (_, location, kept) = start(&server).await;
    let reply = callback(&server, &fake.approve(&location), &kept).await;
    let finish = server.get(&finish_path(&server, &reply)).send().await;
    assert_eq!(finish.status, 200);
    let cookies = finish.all("set-cookie");
    assert_eq!(cookies.len(), 1);
    for part in ["Path=/", "HttpOnly", "SameSite=Strict"] {
        assert!(
            cookies[0].split("; ").any(|piece| piece == part),
            "{part} in {}",
            cookies[0]
        );
    }
    assert!(
        finish
            .body
            .contains(&format!("content=\"0;url=/pages/{HOME}\""))
    );
    let session = cookie_pair(&finish, "hangar_session").unwrap();
    let page = server
        .get(&format!("/pages/{HOME}"))
        .cookie(&session)
        .send()
        .await;
    assert_eq!(page.status, 200, "{}", page.body);
}

#[tokio::test]
async fn a_hand_over_code_works_once() {
    let (server, fake, _clock) = oidc_server().await;
    let (_, location, kept) = start(&server).await;
    let reply = callback(&server, &fake.approve(&location), &kept).await;
    let path = finish_path(&server, &reply);
    assert_eq!(server.get(&path).send().await.status, 200);
    let again = server.get(&path).send().await;
    assert_eq!(
        (again.status, again.code().as_deref()),
        (403, Some("oidc::handover_unknown"))
    );
}

#[tokio::test]
async fn a_hand_over_code_works_for_60_seconds() {
    let (server, fake, _clock) = oidc_server().await;
    let (_, location, kept) = start(&server).await;
    let late = finish_path(
        &server,
        &callback(&server, &fake.approve(&location), &kept).await,
    );
    let (_, location, kept) = start(&server).await;
    let in_time = finish_path(
        &server,
        &callback(&server, &fake.approve(&location), &kept).await,
    );
    server.time.advance(Duration::from_secs(59));
    assert_eq!(server.get(&in_time).send().await.status, 200);
    server.time.advance(Duration::from_secs(2));
    let reply = server.get(&late).send().await;
    assert_eq!(
        (reply.status, reply.code().as_deref()),
        (403, Some("oidc::handover_unknown"))
    );
}

#[tokio::test]
async fn a_callback_completes_beside_a_foreign_cookie_with_a_non_ascii_byte() {
    let (server, fake, _clock) = oidc_server().await;
    let (_, location, kept) = start(&server).await;
    let reply = callback(
        &server,
        &fake.approve(&location),
        &format!("theme=caf\u{e9}; {kept}; other=\u{1F600}"),
    )
    .await;
    assert_eq!(reply.status, 303, "{}", reply.body);
    finish_path(&server, &reply);
    let (_, location, kept) = start(&server).await;
    let plain = callback(&server, &fake.approve(&location), &kept).await;
    assert_eq!(plain.status, 303, "{}", plain.body);
}

#[tokio::test]
async fn the_callback_budget_holds_on_the_sign_in_host_four_reach_the_provider_and_eight_wait() {
    let wait = Duration::from_secs(2);
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect_with_deadline(&fake, Duration::from_secs(30)).await;
    let server = Server::start_with(Options {
        oidc: Some(authenticator),
        limits: Some(limits(|limits| {
            limits.token_wait = wait;
            limits.token_waiters = 8;
        })),
        ..Options::default()
    })
    .await;
    let (_, location, kept) = start(&server).await;
    let query = fake.approve(&location);
    fake.token_reply(TokenReply::Hold);
    let began = Instant::now();
    let mut callbacks = JoinSet::new();
    for _ in 0..20 {
        let call = server
            .oidc_request("GET", &format!("/auth/oidc/callback?{query}"))
            .cookie(&kept);
        callbacks.spawn(async move {
            let reply = call.send().await;
            (reply, began.elapsed())
        });
    }
    let (mut at_once, mut after_wait) = (0, 0);
    while at_once + after_wait < 16 {
        let (reply, took) = callbacks.join_next().await.unwrap().unwrap();
        assert_eq!(
            (reply.status, reply.code().as_deref()),
            (503, Some("server::busy")),
            "a held callback answered before the provider was released"
        );
        if took < wait {
            at_once += 1;
        } else {
            after_wait += 1;
        }
    }
    assert_eq!((at_once, after_wait), (8, 8));
    assert_eq!(fake.token_requests(), 4);
    fake.token_reply(TokenReply::Normal);
    let mut held = Vec::new();
    while let Some(joined) = callbacks.join_next().await {
        held.push(joined.unwrap().0.status);
    }
    held.sort_unstable();
    assert_eq!(held, [303, 403, 403, 403]);
}

#[tokio::test]
async fn a_temporary_refusal_answers_503_and_a_wrong_state_answers_403() {
    let (server, fake, _clock) = oidc_server().await;
    let (_, location, kept) = start(&server).await;
    fake.token_reply(TokenReply::Status(500));
    let reply = callback(&server, &fake.approve(&location), &kept).await;
    assert_eq!(
        (reply.status, reply.code().as_deref()),
        (503, Some("oidc::unavailable"))
    );
    assert!(cookie_pair(&reply, "hangar_session").is_none());
    let (_, location, _) = start(&server).await;
    let wrong = callback(
        &server,
        &fake.approve(&location),
        "hangar_oidc_state=not-it",
    )
    .await;
    assert_eq!(
        (wrong.status, wrong.code().as_deref()),
        (403, Some("oidc::refused"))
    );
}

#[tokio::test]
async fn the_sign_in_host_serves_only_its_two_paths_and_checks_its_host() {
    let (server, _fake, _clock) = oidc_server().await;
    for path in [
        "/up",
        "/hangar/",
        "/launch/probe?nonce=x",
        "/auth/oidc/finish?code=x",
    ] {
        let reply = server.oidc_request("GET", path).send().await;
        assert_eq!(reply.status, 404, "{path} on localhost");
    }
    let wrong_host = server
        .oidc_request("GET", "/auth/oidc/start?return_to=/hangar/")
        .host(&server.authority)
        .send()
        .await;
    assert_eq!(
        (wrong_host.status, wrong_host.code().as_deref()),
        (403, Some("request::host"))
    );
    let other_return = server
        .oidc_request("GET", "/auth/oidc/start?return_to=https://example.com/")
        .send()
        .await;
    assert_eq!(other_return.status, 400);
}

#[tokio::test]
async fn an_oidc_session_reads_only_what_its_allow_list_reaches() {
    let (server, fake, _clock) = oidc_server().await;
    server.seed_page_with("elsewhere", "<p>x</p>", &[]).await;
    let session = sign_in(&server, &fake).await;
    assert_eq!(
        server
            .get(&format!("/pages/{HOME}"))
            .cookie(&session)
            .send()
            .await
            .status,
        200
    );
    let denied = server.get("/pages/elsewhere").cookie(&session).send().await;
    assert_eq!(
        (denied.status, denied.code().as_deref()),
        (403, Some("auth::denied"))
    );
}

//! The hosted site's sign-in: the exact cookies of start and callback, the
//! kept value read beside foreign cookies, session replacement, the
//! session limits, the callback budget and the refusals that set no
//! session.

use crate::fake_provider::{HOME, TokenReply};
use crate::harness::{cookie_pair, limits};
use crate::hosted::{Admits, Hosted, KEPT, SESSION};
use std::time::{Duration, Instant};
use tokio::task::JoinSet;

#[tokio::test]
async fn a_hosted_server_signs_a_person_in_with_exactly_these_cookies() {
    let hosted = Hosted::start().await;
    let (started, location, kept) = hosted.start_sign_in().await;
    assert!(location.as_str().starts_with(hosted.fake.issuer()));
    let set = started.all("set-cookie");
    assert_eq!(set.len(), 1, "{set:?}");
    let value = kept.split_once('=').unwrap().1;
    assert_eq!(
        set[0],
        format!("{KEPT}={value}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=600")
    );
    let callback = hosted
        .callback(&hosted.fake.approve(&location))
        .cookie(&kept)
        .send()
        .await;
    assert_eq!(callback.status, 303, "{}", callback.body);
    assert_eq!(
        callback.header("location"),
        Some(&*format!("/pages/{HOME}"))
    );
    let cookies = callback.all("set-cookie");
    assert_eq!(cookies.len(), 2, "{cookies:?}");
    assert!(
        cookies.contains(
            &format!("{KEPT}=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0").as_str()
        )
    );
    let session = cookies
        .iter()
        .find(|cookie| cookie.starts_with(&format!("{SESSION}=")))
        .expect("a session cookie");
    let parts: Vec<&str> = session.split("; ").collect();
    assert_eq!(
        parts[1..],
        [
            "Path=/",
            "Secure",
            "HttpOnly",
            "SameSite=Lax",
            &format!("Max-Age={}", 12 * 60 * 60)
        ]
    );
    assert!(!session.contains("Domain"));
    let pair = cookie_pair(&callback, SESSION).unwrap();
    assert_eq!(hosted.read_home(&pair).await, 200);
}

#[tokio::test]
async fn a_hosted_sign_in_completes_beside_a_foreign_cookie_with_a_non_ascii_byte() {
    let hosted = Hosted::start().await;
    let session = hosted
        .sign_in_beside(&["theme=caf\u{e9}", "__Host-other=\u{1F600}"])
        .await;
    assert_eq!(hosted.read_home(&session).await, 200);
    let plain = hosted.sign_in().await;
    assert_eq!(hosted.read_home(&plain).await, 200);
}

#[tokio::test]
async fn a_second_sign_in_in_one_browser_ends_the_first_session() {
    let hosted = Hosted::start().await;
    let first = hosted.sign_in().await;
    let elsewhere = hosted.sign_in().await;
    let second = hosted.sign_in_beside(&[&first]).await;
    assert_eq!(hosted.read_home(&first).await, 401);
    assert_eq!(hosted.read_home(&second).await, 200);
    assert_eq!(hosted.read_home(&elsewhere).await, 200);
}

#[tokio::test]
async fn a_principal_with_eight_sessions_who_signs_in_again_in_one_browser_keeps_the_other_seven() {
    let hosted = Hosted::start().await;
    let mut sessions = Vec::new();
    for _ in 0..8 {
        sessions.push(hosted.sign_in().await);
    }
    let replaced = sessions.pop().unwrap();
    let new = hosted.sign_in_beside(&[&replaced]).await;
    assert_eq!(hosted.read_home(&replaced).await, 401);
    assert_eq!(hosted.read_home(&new).await, 200);
    for session in &sessions {
        assert_eq!(hosted.read_home(session).await, 200);
    }
}

#[tokio::test]
async fn a_principal_at_eight_sessions_signs_in_while_the_table_holds_1024() {
    let hosted = Hosted::start_with(Admits::Domain, None).await;
    for _ in 0..8 {
        hosted.sign_in_as("ada").await;
    }
    for at in 0..1016 {
        hosted.sign_in_as(&format!("filler{at}")).await;
    }
    hosted.fake.set_email("grace@example.com");
    let full = hosted.sign_in_reply(&[]).await;
    assert_eq!(
        (full.status, full.code().as_deref()),
        (503, Some("oidc::unavailable"))
    );
    let ada = hosted.sign_in_as("ada").await;
    assert_eq!(hosted.read_home(&ada).await, 200);
}

#[tokio::test]
async fn twenty_parallel_callbacks_take_four_token_requests_and_a_signed_in_read_still_answers() {
    let hosted = Hosted::start_with(
        Admits::AllowList,
        Some(limits(|limits| {
            limits.request_time = Duration::from_secs(20)
        })),
    )
    .await;
    let reader = hosted.sign_in().await;
    let (_, location, kept) = hosted.start_sign_in().await;
    let query = hosted.fake.approve(&location);
    hosted.fake.token_reply(TokenReply::Stall);
    let began = Instant::now();
    let mut callbacks = JoinSet::new();
    for _ in 0..20 {
        let call = hosted.callback(&query).cookie(&kept);
        callbacks.spawn(async move {
            let reply = call.send().await;
            (reply, Instant::now())
        });
    }
    let mut busy = 0;
    let mut held = 0;
    let mut first_held: Option<Instant> = None;
    let mut last_busy: Option<Instant> = None;
    let mut read = None;
    while let Some(joined) = callbacks.join_next().await {
        let (reply, at) = joined.unwrap();
        if reply.code().as_deref() == Some("server::busy") {
            assert_eq!(reply.status, 503);
            busy += 1;
            last_busy = Some(at);
            if busy == 16 {
                read = Some(hosted.read_home(&reader).await);
            }
        } else {
            held += 1;
            first_held.get_or_insert(at);
        }
    }
    assert_eq!((busy, held), (16, 4));
    assert_eq!(
        read,
        Some(200),
        "a signed-in read answers while 4 callbacks wait"
    );
    let last_busy = last_busy.unwrap();
    assert!(
        last_busy.duration_since(began) < Duration::from_secs(1),
        "the busy answers came at once"
    );
    assert!(first_held.unwrap() > last_busy);
    hosted.fake.token_reply(TokenReply::Normal);
    let after = hosted.sign_in().await;
    assert_eq!(hosted.read_home(&after).await, 200);
}

#[tokio::test]
async fn a_failed_callback_leaves_the_browsers_session_working_and_clears_the_kept_state() {
    let hosted = Hosted::start().await;
    let session = hosted.sign_in().await;
    hosted.fake.set_email("eve@example.com");
    let refused = hosted.sign_in_reply(&[&session]).await;
    assert_eq!(
        (refused.status, refused.code().as_deref()),
        (403, Some("oidc::refused"))
    );
    assert!(cookie_pair(&refused, SESSION).is_none());
    assert!(
        refused.all("set-cookie").contains(
            &format!("{KEPT}=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0").as_str()
        )
    );
    assert_eq!(hosted.read_home(&session).await, 200);
    hosted.fake.set_email("ada@example.com");
    let replaced = hosted.sign_in_beside(&[&session]).await;
    assert_eq!(hosted.read_home(&session).await, 401);
    assert_eq!(hosted.read_home(&replaced).await, 200);
}

#[tokio::test]
async fn a_callback_without_the_kept_cookie_or_with_another_browsers_sets_no_session() {
    let hosted = Hosted::start().await;
    let (_, location, kept) = hosted.start_sign_in().await;
    let query = hosted.fake.approve(&location);
    let bare = hosted.callback(&query).send().await;
    assert_eq!(
        (bare.status, bare.code().as_deref()),
        (403, Some("oidc::refused"))
    );
    assert!(cookie_pair(&bare, SESSION).is_none());
    let (_, _, other) = hosted.start_sign_in().await;
    let crossed = hosted.callback(&query).cookie(&other).send().await;
    assert_eq!(
        (crossed.status, crossed.code().as_deref()),
        (403, Some("oidc::refused"))
    );
    assert!(cookie_pair(&crossed, SESSION).is_none());
    let own = hosted.callback(&query).cookie(&kept).send().await;
    assert_eq!(own.status, 303, "{}", own.body);
    assert!(cookie_pair(&own, SESSION).is_some());
}

#[tokio::test]
async fn a_sign_in_started_before_a_thousand_further_starts_still_completes() {
    let hosted = Hosted::start().await;
    let (_, location, kept) = hosted.start_sign_in().await;
    for _ in 0..1000 {
        hosted.start_sign_in().await;
    }
    let reply = hosted
        .callback(&hosted.fake.approve(&location))
        .cookie(&kept)
        .send()
        .await;
    assert_eq!(reply.status, 303, "{}", reply.body);
    let session = cookie_pair(&reply, SESSION).unwrap();
    assert_eq!(hosted.read_home(&session).await, 200);
}

#[tokio::test]
async fn a_replayed_callback_is_refused_and_the_first_one_signed_in() {
    let hosted = Hosted::start().await;
    let (_, location, kept) = hosted.start_sign_in().await;
    let query = hosted.fake.approve(&location);
    let first = hosted.callback(&query).cookie(&kept).send().await;
    assert_eq!(first.status, 303);
    let again = hosted.callback(&query).cookie(&kept).send().await;
    assert_eq!(
        (again.status, again.code().as_deref()),
        (403, Some("oidc::refused"))
    );
    assert!(cookie_pair(&again, SESSION).is_none());
    assert_eq!(hosted.fake.token_requests(), 2);
}

#[tokio::test]
async fn a_start_with_a_return_path_off_this_site_is_refused_and_a_local_path_is_admitted() {
    let hosted = Hosted::start().await;
    for back in [
        "//evil.example",
        "/%5Cevil.example",
        "https://evil.example",
        "/%2F%2Fevil.example",
    ] {
        let reply = hosted
            .get(&format!("/auth/oidc/start?return_to={back}"))
            .send()
            .await;
        assert_eq!(
            (reply.status, reply.code().as_deref()),
            (400, Some("oidc::bad_return")),
            "{back}"
        );
        assert!(reply.all("set-cookie").is_empty());
    }
    let (reply, _, _) = hosted.start_sign_in().await;
    assert_eq!(reply.status, 303);
}

//! The hosted site's sign-in: the exact cookies of start and callback, the
//! kept value read beside foreign cookies, session replacement, the
//! session limits, the callback budget and the refusals that set no
//! session.

use crate::fake_provider::{HOME, TokenReply, form};
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
            &format!("Max-Age={}", 7 * 24 * 60 * 60)
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
async fn twenty_parallel_callbacks_take_four_token_requests_eight_wait_and_a_signed_in_read_still_answers()
 {
    let wait = Duration::from_secs(2);
    let hosted = Hosted::start_with_deadline(
        Duration::from_secs(30),
        limits(|limits| {
            limits.token_wait = wait;
            limits.token_waiters = 8;
        }),
    )
    .await;
    let reader = hosted.sign_in().await;
    let (_, location, kept) = hosted.start_sign_in().await;
    let query = hosted.fake.approve(&location);
    let before = hosted.fake.token_requests();
    hosted.fake.token_reply(TokenReply::Hold);
    let began = Instant::now();
    let mut callbacks = JoinSet::new();
    for _ in 0..20 {
        let call = hosted.callback(&query).cookie(&kept);
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
    assert_eq!(hosted.fake.token_requests() - before, 4);
    assert_eq!(
        hosted.read_home(&reader).await,
        200,
        "a signed-in read answers while 4 callbacks hold token requests"
    );
    hosted.fake.token_reply(TokenReply::Normal);
    let mut held = Vec::new();
    while let Some(joined) = callbacks.join_next().await {
        held.push(joined.unwrap().0.status);
    }
    held.sort_unstable();
    assert_eq!(held, [303, 403, 403, 403]);
    let after = hosted.sign_in().await;
    assert_eq!(hosted.read_home(&after).await, 200);
}

#[tokio::test]
async fn a_callback_that_finds_every_slot_taken_by_replays_waits_and_signs_in_when_one_frees() {
    let hosted = Hosted::start_with_deadline(Duration::from_secs(30), limits(|_| {})).await;
    let (_, junk_location, junk_kept) = hosted.start_sign_in().await;
    let state = junk_location
        .query_pairs()
        .find(|(name, _)| name == "state")
        .map(|(_, value)| value.into_owned())
        .unwrap();
    let (_, location, kept) = hosted.start_sign_in().await;
    let query = hosted.fake.approve(&location);
    hosted.fake.token_reply(TokenReply::Hold);
    let mut replays = JoinSet::new();
    for at in 0..4 {
        let junk = form(&[("code", &format!("junk-{at}")), ("state", &state)]);
        let call = hosted.callback(&junk).cookie(&junk_kept);
        replays.spawn(call.send());
    }
    let held = Instant::now();
    while hosted.fake.token_requests() < 4 {
        assert!(
            held.elapsed() < Duration::from_secs(10),
            "the replays reach the provider"
        );
        tokio::task::yield_now().await;
    }
    let own = hosted.callback(&query).cookie(&kept);
    let mut waiting = JoinSet::new();
    waiting.spawn(own.send());
    let waited = tokio::time::timeout(Duration::from_millis(500), waiting.join_next()).await;
    assert!(waited.is_err(), "the callback waits for a slot");
    assert_eq!(hosted.fake.token_requests(), 4);
    hosted.fake.token_reply(TokenReply::Normal);
    let signed_in = waiting.join_next().await.unwrap().unwrap();
    assert_eq!(signed_in.status, 303, "{}", signed_in.body);
    let session = cookie_pair(&signed_in, SESSION).unwrap();
    assert_eq!(hosted.read_home(&session).await, 200);
    while let Some(replay) = replays.join_next().await {
        let replay = replay.unwrap();
        assert_eq!(
            (replay.status, replay.code().as_deref()),
            (403, Some("oidc::refused"))
        );
    }
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

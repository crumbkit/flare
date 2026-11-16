//! Plan test 15: no secret reaches the log or a `Debug` output. The test
//! makes every kind of secret first, including an OpenID Connect sign-in
//! against the fake provider whose client secret is a marker, and formats
//! the app's `Debug` output after them.
//!
//! This test is the only one in its binary. It captures the log with a
//! subscriber that `tracing::subscriber::set_default` sets on its own
//! thread. While that subscriber is the only one registered, tracing-core
//! works out a new callsite's interest from the default subscriber of the
//! thread that reaches the callsite first. When another test's thread in
//! the same binary reaches the request log first, it has no subscriber, so
//! the callsite is cached as never enabled and this test sees no log. A
//! binary of its own leaves no other thread to reach a callsite first.
#![cfg(test)]

#[path = "../../hangar-auth-oidc/tests/support/fake_provider.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "the fake provider is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod fake_provider;

#[path = "../../hangar-auth-loopback/tests/support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch directory is hangar-auth-loopback's test support, which panics on a broken fixture"
)]
pub mod scratch;

#[path = "support/harness.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the harness is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod harness;

use fake_provider::{CLIENT_SECRET, Fake, connect};
use harness::{BINDING, Options, Server, cookie_pair, form_pairs, value};
use std::io::Write;
use std::sync::{Arc, Mutex};
use url::Url;

/// The secrets of an OpenID Connect sign-in: the kept-state cookie's
/// value, the hand-over code and the session.
struct OidcSecrets {
    kept: String,
    handover: String,
    session: String,
}

/// Signs a browser in through the fake and returns the secrets the
/// sign-in made.
async fn oidc_sign_in(server: &Server, fake: &Fake) -> OidcSecrets {
    let started = server
        .oidc_request("GET", "/auth/oidc/start?return_to=/hangar/")
        .send()
        .await;
    assert_eq!(started.status, 303, "{}", started.body);
    let location = Url::parse(started.header("location").unwrap()).unwrap();
    let kept = cookie_pair(&started, "hangar_oidc_state").unwrap();
    let called = server
        .oidc_request(
            "GET",
            &format!("/auth/oidc/callback?{}", fake.approve(&location)),
        )
        .cookie(&kept)
        .send()
        .await;
    assert_eq!(called.status, 303, "{}", called.body);
    let finish = Url::parse(called.header("location").unwrap()).unwrap();
    let handover = finish
        .query_pairs()
        .find(|(name, _)| name == "code")
        .map(|(_, code)| code.into_owned())
        .unwrap();
    let finished = server
        .get(&format!("{}?{}", finish.path(), finish.query().unwrap()))
        .send()
        .await;
    assert_eq!(finished.status, 200, "{}", finished.body);
    let session = cookie_pair(&finished, "hangar_session").unwrap();
    OidcSecrets {
        kept: kept.trim_start_matches("hangar_oidc_state=").to_owned(),
        handover,
        session: session.trim_start_matches("hangar_session=").to_owned(),
    }
}

/// A log writer that keeps every byte for the test to read.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "current_thread")]
async fn no_secret_reaches_the_log_or_a_debug_output() {
    let log = Captured::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let server = Server::start_with(Options {
        oidc: Some(authenticator),
        ..Options::default()
    })
    .await;
    server.seed_binding().await;
    let oidc = oidc_sign_in(&server, &fake).await;
    let token = server.token();
    let cookie = server.sign_in().await;
    let session = cookie.trim_start_matches("hangar_session=").to_owned();
    let drawn = server
        .get(&format!("/b/{BINDING}"))
        .cookie(&cookie)
        .send()
        .await;
    assert_eq!(drawn.status, 200, "{}", drawn.body);
    let pairs = form_pairs(&drawn.body);
    let form_token = value(&pairs, "token").to_owned();
    let codes = server
        .request("POST", "/launch/codes?return_to=/hangar/")
        .bearer(&token)
        .send()
        .await;
    assert_eq!(codes.status, 200);
    server
        .get("/launch?code=a-wrong-code-that-is-not-logged")
        .send()
        .await;
    let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    assert!(
        text.contains("status=200"),
        "the request log is written: {text}"
    );
    let debug = server.debug_now();
    assert!(debug.contains("App"), "{debug}");
    for secret in [
        token.as_str(),
        session.as_str(),
        form_token.as_str(),
        "a-wrong-code-that-is-not-logged",
        CLIENT_SECRET,
        oidc.kept.as_str(),
        oidc.handover.as_str(),
        oidc.session.as_str(),
    ] {
        assert!(!text.contains(secret), "the log holds a secret: {text}");
        for output in [&server.debug, &debug] {
            assert!(!output.contains(secret), "Debug holds a secret: {output}");
        }
        assert!(!format!("{:?}", server.loopback).contains(secret));
    }
    assert!(!text.contains("code="), "the log holds a query string");
    assert!(
        !server.debug.contains(&server.authority),
        "Debug holds the launch host"
    );
    assert!(
        !text.contains(&server.authority),
        "the log holds the launch host"
    );
}

//! Plan test 15: no secret reaches the log or a `Debug` output.
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

use harness::{BINDING, Server, form_pairs, value};
use std::io::Write;
use std::sync::{Arc, Mutex};

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
    let server = Server::start().await;
    server.seed_binding().await;
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
    for secret in [
        token.as_str(),
        session.as_str(),
        form_token.as_str(),
        "a-wrong-code-that-is-not-logged",
    ] {
        assert!(!text.contains(secret), "the log holds a secret: {text}");
        assert!(
            !server.debug.contains(secret),
            "Debug holds a secret: {}",
            server.debug
        );
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

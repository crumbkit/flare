//! A hosted server's log: a refused callback logs its refusal, and no state,
//! kept value, session, code or client secret reaches the log.
//!
//! This test is the only one in its binary, for the reason tests/log.rs
//! gives: a callsite's interest is worked out from the thread that reaches
//! it first, so no other test's thread may reach the request log first.
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

#[path = "support/hosted.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the hosted harness is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod hosted;

use fake_provider::CLIENT_SECRET;
use harness::cookie_pair;
use hosted::{Hosted, KEPT, SESSION};
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

fn value_of(pair: &str) -> String {
    pair.split_once('=').unwrap().1.to_owned()
}

#[tokio::test(flavor = "current_thread")]
async fn a_refused_hosted_callback_logs_its_refusal_and_no_secret_and_a_good_one_logs_none() {
    let log = Captured::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let hosted = Hosted::start().await;
    let session = hosted.sign_in().await;
    let good = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    assert!(good.contains("status=303"), "{good}");
    assert!(
        !good.contains("sign-in was refused"),
        "a good sign-in logs no refusal: {good}"
    );
    hosted.fake.set_email("eve@example.com");
    let (_, location, kept) = hosted.start_sign_in().await;
    let query = hosted.fake.approve(&location);
    let refused = hosted
        .callback(&query)
        .cookie(&format!("{kept}; {session}"))
        .send()
        .await;
    assert_eq!(refused.code().as_deref(), Some("oidc::refused"));
    assert!(cookie_pair(&refused, SESSION).is_none());
    assert_eq!(hosted.read_home(&session).await, 200);
    let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains("sign-in was refused"), "{text}");
    assert!(text.contains("oidc::refused"), "{text}");
    assert!(text.contains("NotAllowed"), "{text}");
    let state = location
        .query_pairs()
        .find(|(name, _)| name == "state")
        .map(|(_, state)| state.into_owned())
        .unwrap();
    let code = url::form_urlencoded::parse(query.as_bytes())
        .find(|(name, _)| name == "code")
        .map(|(_, code)| code.into_owned())
        .unwrap();
    for secret in [
        state,
        code,
        value_of(&kept),
        value_of(&session),
        CLIENT_SECRET.to_owned(),
        "eve@example.com".to_owned(),
    ] {
        assert!(!text.contains(&secret), "the log holds a secret: {text}");
    }
    assert!(!text.contains(KEPT), "the log names the cookie: {text}");
}

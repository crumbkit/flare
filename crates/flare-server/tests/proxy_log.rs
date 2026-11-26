//! A hosted server behind a single sign-on proxy logs which check refused a
//! proxy's token, and never the token or the address it names.
//!
//! This test is the only one in its binary, for the reason tests/log.rs
//! gives: a callsite's interest is worked out from the thread that reaches
//! it first, so no other test's thread may reach the log first.
#![cfg(test)]

#[path = "../../flare-auth-oidc/tests/support/fake_proxy.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the fake proxy issuer is test support: it panics on a broken fixture, and this test uses part of it"
)]
pub mod fake_proxy;

#[path = "../../flare-auth-loopback/tests/support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch directory is flare-auth-loopback's test support, which panics on a broken fixture"
)]
pub mod scratch;

#[path = "support/harness.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    dead_code,
    reason = "the harness is test support: it panics on a broken fixture, and this test uses part of it"
)]
pub mod harness;

#[path = "support/faulty.rs"]
pub mod faulty;

#[path = "support/proxied.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the proxied harness is test support: it panics on a broken fixture, and this test uses part of it"
)]
pub mod proxied;

use fake_proxy::{EMAIL, HOME, Sign};
use proxied::Proxied;
use serde_json::json;
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
async fn a_refused_proxy_token_logs_its_check_and_no_token_and_a_good_one_logs_none() {
    let log = Captured::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let site = Proxied::start().await;
    let good = site.fake.token(Sign::Rs256);
    let read = site
        .get(&format!("/pages/{HOME}"))
        .cookie(&Proxied::cookie(&good))
        .send()
        .await;
    assert_eq!(read.status, 200);
    let before = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    assert!(!before.contains("refused a credential"), "{before}");
    let misdirected = site.fake.token_with(Sign::Rs256, |claims| {
        claims.insert("aud".into(), json!("another-app"));
    });
    let refused = site
        .get(&format!("/pages/{HOME}"))
        .cookie(&Proxied::cookie(&misdirected))
        .send()
        .await;
    assert_eq!(refused.code().as_deref(), Some("auth::proxy_refused"));
    let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains("refused a credential"), "{text}");
    assert!(
        text.contains("check=\"audience\"") || text.contains("check=audience"),
        "{text}"
    );
    for secret in [good, misdirected, EMAIL.to_owned()] {
        assert!(!text.contains(&secret), "the log holds a secret: {text}");
    }
}

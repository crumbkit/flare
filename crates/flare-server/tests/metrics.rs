//! A hosted server's metric lines: with metrics off it writes none and its
//! `served` lines are what they were; with metrics on, a run of sign-ins,
//! a page view, a token exchange, a share and refusals writes one EMF line
//! for each event, `Totals` counts them as the run made them, and no line
//! holds an address, a token, a code, a session, a share id or a title. A
//! log that refuses every write changes no answer.
//!
//! This test is the only one in its binary, for the reason tests/log.rs
//! gives: a callsite's interest is worked out from the thread that reaches
//! it first, so no other test's thread may reach the request log first.
#![cfg(test)]

#[path = "../../flare-auth-oidc/tests/support/fake_provider.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "the fake provider is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod fake_provider;

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

#[path = "support/faulty.rs"]
pub mod faulty;

#[path = "support/api.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the API harness is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod api;

use api::Api;
use fake_provider::CLIENT_SECRET;
use flare_server::{Metrics, Totals};
use serde_json::Value;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// A log writer that keeps every byte for the test to read, or refuses
/// every write while `closed` is set, as a closed standard error would.
#[derive(Clone, Default)]
struct Captured {
    bytes: Arc<Mutex<Vec<u8>>>,
    closed: Arc<AtomicBool>,
}

impl Captured {
    /// The text written since byte `from`.
    fn since(&self, from: usize) -> String {
        String::from_utf8(self.bytes.lock().unwrap()[from..].to_vec()).unwrap()
    }

    fn len(&self) -> usize {
        self.bytes.lock().unwrap().len()
    }
}

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
        }
        self.bytes.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The JSON of each metric line in `text`.
fn metric_lines(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.contains("flare::metrics"))
        .map(|line| line[line.find('{').unwrap()..].to_owned())
        .collect()
}

/// Each `served` line of `text` without its time, request id and
/// milliseconds, which differ between runs.
fn served_shapes(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.contains(" served "))
        .map(|line| {
            line[line.find("INFO").unwrap()..]
                .split(' ')
                .filter(|field| !field.starts_with("request=") && !field.starts_with("ms="))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// The same requests on either server: alice signs in and reads the page
/// `home`.
async fn sign_in_and_read(api: &Api) -> String {
    let cookie = api.session("alice").await;
    assert_eq!(api.hosted.read_home(&cookie).await, 200);
    cookie
}

#[tokio::test(flavor = "current_thread")]
async fn metric_lines_count_a_whole_run_hold_no_personal_value_and_never_change_an_answer() {
    let log = Captured::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    // With metrics off, nothing is counted and the served lines stay.
    let off = Api::start().await;
    let from = log.len();
    sign_in_and_read(&off).await;
    let off_text = log.since(from);
    assert!(
        metric_lines(&off_text).is_empty(),
        "metrics off writes no metric line: {off_text}"
    );
    assert!(!off_text.contains("_aws"), "{off_text}");
    let off_served = served_shapes(&off_text);
    assert!(!off_served.is_empty(), "{off_text}");

    // With metrics on, every event writes its line.
    let api = Api::start_with(|app| app.with_metrics(Metrics::new(true))).await;
    let from = log.len();
    let cookie = sign_in_and_read(&api).await;
    let on_served = served_shapes(&log.since(from));
    assert_eq!(
        on_served, off_served,
        "the served lines are the same with metrics on"
    );
    let code = api.code_for("alice").await;
    let (status, issued) = api.exchange(&code, "test").await;
    assert_eq!(status, 201, "{issued}");
    let token = issued["token"].as_str().unwrap().to_owned();
    let title = "Quarterly plan for the kitchen";
    let id = api.share(&token, title, &["bob@example.com"], &[]).await;
    let (status, _) = api.exchange("not-a-login-code", "test").await;
    assert!((400..500).contains(&status), "{status}");
    let (status, _) = api
        .read("GET", "/api/shares?scope=mine", Some("flare_not-a-token"))
        .await;
    assert_eq!(status, 401);
    let refused = api
        .hosted
        .callback("state=abcdefabcdefabcdef&code=a-code-that-is-not-logged")
        .send()
        .await;
    assert_eq!(refused.code().as_deref(), Some("oidc::refused"));

    let text = log.since(from);
    let lines = metric_lines(&text);
    let mut totals = Totals::default();
    for line in &lines {
        totals.add(line);
    }
    assert_eq!(totals.skipped(), 0, "{text}");
    let served = u64::try_from(text.matches(" served ").count()).unwrap();
    let counted: Vec<(&str, u64)> = totals.by_metric().collect();
    assert_eq!(
        counted,
        [
            ("PagesViewed", 1),
            ("Requests", served),
            ("SharesCreated", 1),
            ("SignIns", 2),
            ("SignInsRefused", 1),
            ("TokensExchanged", 1),
            ("TokensRefused", 2),
        ],
        "{text}"
    );
    let reasons: Vec<Value> = lines
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|value| value.get("SignInsRefused").is_some())
        .map(|value| value["Reason"].clone())
        .collect();
    assert_eq!(reasons, ["no_kept_state"]);
    for line in &lines {
        let value: Value = serde_json::from_str(line).unwrap();
        for key in value.as_object().unwrap().keys() {
            assert!(
                [
                    "_aws",
                    "Route",
                    "StatusClass",
                    "Reason",
                    "Requests",
                    "SharesCreated",
                    "PagesViewed",
                    "SignIns",
                    "SignInsRefused",
                    "TokensExchanged",
                    "TokensRefused",
                ]
                .contains(&key.as_str()),
                "{line}"
            );
        }
        for value in [
            "alice@example.com",
            "bob@example.com",
            "example.com",
            token.as_str(),
            code.as_str(),
            cookie.split_once('=').unwrap().1,
            CLIENT_SECRET,
            "a-code-that-is-not-logged",
            "abcdefabcdefabcdef",
            "flare_not-a-token",
            id.as_str(),
            title,
            "/pages/home",
            "127.0.0.1",
            api.hosted.authority.as_str(),
        ] {
            assert!(!line.contains(value), "a metric line holds {value}: {line}");
        }
    }

    // A log that refuses every write changes no answer.
    log.closed.store(true, Ordering::SeqCst);
    let reply = api.hosted.read_home_reply(&cookie).await;
    log.closed.store(false, Ordering::SeqCst);
    assert_eq!(reply.status, 200, "{}", reply.body);
}

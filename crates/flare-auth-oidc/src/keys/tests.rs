//! Unit tests of the key cache's fetches: a sign-in that waits for another
//! sign-in's fetch, and a sign-in dropped during its own fetch.
//!
//! Each test polls the sign-ins by hand, so the second sign-in is waiting on
//! the cache before the first one's fetch answers.

use super::*;
use aws_lc_rs::rsa::{KeyPair, KeySize};
use aws_lc_rs::signature::KeyPair as _;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::json;
use std::cell::Cell;
use std::pin::{Pin, pin};
use std::task::{Context, Waker};
use std::time::UNIX_EPOCH;
use tokio::sync::watch;

/// The reply that a held fetch gives once a test sets it.
type Answer = Result<Vec<u8>, ReplyError>;

/// A key set endpoint whose replies wait until the test sets one, and which
/// counts the fetches it receives.
struct Endpoint {
    fetches: Cell<usize>,
    answer: watch::Sender<Option<Answer>>,
}

impl Endpoint {
    fn held() -> Self {
        Self {
            fetches: Cell::new(0),
            answer: watch::Sender::new(None),
        }
    }

    fn answering(answer: Answer) -> Self {
        let endpoint = Self::held();
        endpoint.answer(answer);
        endpoint
    }

    /// Answers every held fetch, and each later one, with `answer`.
    fn answer(&self, answer: Answer) {
        self.answer.send_replace(Some(answer));
    }

    fn fetch(&self) -> impl Future<Output = Answer> + use<> {
        self.fetches.set(self.fetches.get() + 1);
        let mut answer = self.answer.subscribe();
        async move {
            let answered = answer.wait_for(Option::is_some).await;
            answered
                .ok()
                .and_then(|answer| answer.clone())
                .unwrap_or(Err(ReplyError::Unreachable))
        }
    }
}

/// The key set body that publishes one RSA key under each of `kids`.
fn key_set(kids: &[&str]) -> Vec<u8> {
    let key = KeyPair::generate(KeySize::Rsa2048).unwrap();
    let public = key.public_key();
    let n = URL_SAFE_NO_PAD.encode(public.modulus().big_endian_without_leading_zero());
    let e = URL_SAFE_NO_PAD.encode(public.exponent().big_endian_without_leading_zero());
    let keys: Vec<_> = kids
        .iter()
        .map(
            |kid| json!({ "kty": "RSA", "use": "sig", "alg": "RS256", "kid": kid, "n": n, "e": e }),
        )
        .collect();
    json!({ "keys": keys }).to_string().into_bytes()
}

fn start() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

/// A cache that holds the key `old`, fetched at [`start`], and keeps its
/// held keys when a fetch fails, as the proxy sign-in's does.
fn cache() -> KeyCache {
    let rule = KeyRule {
        keeps_held_keys: true,
        ..KeyRule::OIDC
    };
    KeyCache::under(KeySet::parse(&key_set(&["old"])).unwrap(), start(), rule)
}

/// Polls `sign_in` once and says whether it is still waiting.
fn waits<F: Future>(sign_in: Pin<&mut F>) -> bool {
    sign_in
        .poll(&mut Context::from_waker(Waker::noop()))
        .is_pending()
}

/// Runs two sign-ins for `kid` at `now`: the first one's fetch is held
/// until the second one waits on the cache, and then answers `answer`.
/// Returns what each found and how many fetches the endpoint received.
async fn two_sign_ins(
    kid: &str,
    now: SystemTime,
    answer: Answer,
) -> (Option<KeyError>, Option<KeyError>, usize) {
    let cache = cache();
    let endpoint = Endpoint::held();
    let mut first = pin!(cache.key_with(kid, now, || endpoint.fetch()));
    let mut second = pin!(cache.key_with(kid, now, || endpoint.fetch()));
    assert!(waits(first.as_mut()), "the first sign-in fetches");
    assert!(waits(second.as_mut()), "the second sign-in waits");
    endpoint.answer(answer);
    let (first, second) = tokio::join!(first, second);
    (first.err(), second.err(), endpoint.fetches.get())
}

#[tokio::test]
async fn a_sign_in_that_waits_for_a_failed_fetch_gets_its_failure_and_fetches_nothing() {
    let stale = start() + KEY_SET_LIFETIME;
    let failures = [
        (Err(ReplyError::Unreachable), KeyError::Unavailable),
        (Ok(br#"{"keys":[]}"#.to_vec()), KeyError::Unusable),
    ];
    for (answer, failure) in failures {
        // A new key id gets the failure, and a key id in the key set held
        // for over one hour keeps its held key.
        for (kid, now, found) in [("new", start(), Some(failure)), ("old", stale, None)] {
            let (first, second, fetches) = two_sign_ins(kid, now, answer.clone()).await;
            assert_eq!((first, second), (found, found), "{kid}");
            assert_eq!(fetches, 1, "{kid}");
        }
    }
}

#[tokio::test]
async fn a_sign_in_that_waits_for_a_successful_fetch_uses_its_key_set_and_fetches_nothing() {
    let stale = start() + KEY_SET_LIFETIME;
    for (kid, now) in [("new", start()), ("old", stale)] {
        let answer = Ok(key_set(&["old", "new"]));
        assert_eq!(
            two_sign_ins(kid, now, answer).await,
            (None, None, 1),
            "{kid}"
        );
    }
    // A fetch that succeeds without the key id refuses both sign-ins as an
    // unknown key, after one fetch.
    let answer = Ok(key_set(&["old"]));
    let unknown = Some(KeyError::Unknown);
    assert_eq!(
        two_sign_ins("new", start(), answer).await,
        (unknown, unknown, 1)
    );
}

#[tokio::test]
async fn a_sign_in_dropped_during_its_fetch_leaves_the_next_sign_in_free_to_fetch_at_once() {
    let cache = cache();
    let held = Endpoint::held();
    {
        let mut dropped = pin!(cache.key_with("new", start(), || held.fetch()));
        assert!(waits(dropped.as_mut()), "the sign-in fetches");
    }
    let endpoint = Endpoint::answering(Ok(key_set(&["old", "new"])));
    let found = cache.key_with("new", start(), || endpoint.fetch()).await;
    assert_eq!(found.err(), None);
    assert_eq!(endpoint.fetches.get(), 1);
}

#[tokio::test]
async fn after_a_failed_fetch_the_next_unknown_key_id_waits_out_the_refresh_interval() {
    let cache = cache();
    let down = Endpoint::answering(Err(ReplyError::Unreachable));
    let failed = cache.key_with("new", start(), || down.fetch()).await;
    assert_eq!(failed.err(), Some(KeyError::Unavailable));
    // Within the interval the last fetch's failure stands, with no fetch.
    let later = cache.key_with("new", start(), || down.fetch()).await;
    assert_eq!(later.err(), Some(KeyError::Unavailable));
    assert_eq!(down.fetches.get(), 1);
    // The admitted twin: once the interval has passed, the next sign-in
    // fetches again.
    let endpoint = Endpoint::answering(Ok(key_set(&["old", "new"])));
    let found = cache
        .key_with("new", start() + REFRESH_INTERVAL, || endpoint.fetch())
        .await;
    assert_eq!(found.err(), None);
    assert_eq!(endpoint.fetches.get(), 1);
}

#[tokio::test]
async fn a_held_set_over_an_hour_old_serves_its_keys_while_fetches_fail_and_fetches_once_a_minute()
{
    let cache = cache();
    let stale = start() + KEY_SET_LIFETIME;
    let down = Endpoint::answering(Err(ReplyError::Unreachable));
    for second in 0..3 {
        let now = stale + Duration::from_secs(second);
        let found = cache.key_with("old", now, || down.fetch()).await;
        assert_eq!(found.err(), None, "second {second}");
    }
    assert_eq!(down.fetches.get(), 1);
    let found = cache
        .key_with("old", stale + REFRESH_INTERVAL, || down.fetch())
        .await;
    assert_eq!(found.err(), None);
    assert_eq!(down.fetches.get(), 2);
    // The admitted twin: a fetch that succeeds replaces the set.
    let endpoint = Endpoint::answering(Ok(key_set(&["new"])));
    let later = stale + REFRESH_INTERVAL + REFRESH_INTERVAL;
    let replaced = cache.key_with("old", later, || endpoint.fetch()).await;
    assert_eq!(replaced.err(), Some(KeyError::Unknown));
    assert_eq!(endpoint.fetches.get(), 1);
}

#[tokio::test]
async fn after_a_successful_fetch_without_the_key_id_the_next_lookup_is_unknown_without_a_fetch() {
    let cache = cache();
    let endpoint = Endpoint::answering(Ok(key_set(&["old"])));
    let first = cache.key_with("new", start(), || endpoint.fetch()).await;
    assert_eq!(first.err(), Some(KeyError::Unknown));
    let second = cache.key_with("new", start(), || endpoint.fetch()).await;
    assert_eq!(second.err(), Some(KeyError::Unknown));
    assert_eq!(endpoint.fetches.get(), 1);
}

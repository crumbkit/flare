//! Unit tests of the sealed kept state and its keys.

use super::*;
use crate::config::ClientId;
use flare_core::memory::MemorySealingKeys;
use flare_core::{SealingKeys, SealingKeysError};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::UNIX_EPOCH;

const CLIENT: &str = "flare-test-client";
const DAY: u64 = 24 * 60 * 60;

/// A time with no fraction of a second, so whole-second arithmetic in the
/// tests is exact.
fn whole_second() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

fn at(base: SystemTime, seconds: u64) -> SystemTime {
    base + Duration::from_secs(seconds)
}

fn pending(return_to: &str, issued: SystemTime) -> Pending {
    Pending::new(ReturnTo::try_from(return_to).unwrap(), issued).unwrap()
}

/// A sealer over `keys`, which is shared when several sealers pass it.
fn sealer_over(keys: &Arc<dyn DynKeys>, client: &str) -> Sealer {
    Sealer::new(Arc::clone(keys), &ClientId::try_from(client).unwrap())
}

fn memory_keys() -> Arc<dyn DynKeys> {
    Arc::new(MemorySealingKeys::new())
}

/// Counts reads of a key by id and delegates to memory keys.
#[derive(Default)]
struct Counting {
    inner: MemorySealingKeys,
    gets: AtomicUsize,
}

impl SealingKeys for Counting {
    async fn current(
        &self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Result<SealingKey, SealingKeysError> {
        SealingKeys::current(&self.inner, fresh, now).await
    }

    async fn get(&self, id: KeyId, now: Timestamp) -> Result<Option<SealingKey>, SealingKeysError> {
        self.gets.fetch_add(1, Ordering::Relaxed);
        SealingKeys::get(&self.inner, id, now).await
    }

    async fn rotate(
        &self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Result<SealingKey, SealingKeysError> {
        SealingKeys::rotate(&self.inner, fresh, now).await
    }
}

/// A key store that cannot answer.
struct Down;

impl SealingKeys for Down {
    async fn current(&self, _: KeyBytes, _: Timestamp) -> Result<SealingKey, SealingKeysError> {
        Err(SealingKeysError::Unavailable)
    }

    async fn get(&self, _: KeyId, _: Timestamp) -> Result<Option<SealingKey>, SealingKeysError> {
        Err(SealingKeysError::Unavailable)
    }

    async fn rotate(&self, _: KeyBytes, _: Timestamp) -> Result<SealingKey, SealingKeysError> {
        Err(SealingKeysError::Unavailable)
    }
}

#[tokio::test]
async fn a_sealed_kept_state_opens_with_the_key_that_sealed_it() {
    let now = whole_second();
    let sealer = sealer_over(&memory_keys(), CLIENT);
    let made = pending("/pages/plan", now);
    let state = made.state.expose_secret().to_owned();
    let kept = sealer.seal(&made, now).await.unwrap();
    let opened = sealer.open(&kept, now).await.unwrap();
    assert_eq!(opened.state.expose_secret(), state);
    assert_eq!(opened.nonce.expose_secret(), made.nonce.expose_secret());
    assert_eq!(
        opened.verifier.expose_secret(),
        made.verifier.expose_secret()
    );
    assert_eq!(opened.return_to.as_str(), "/pages/plan");
    assert!(opened.matches(&state));
    assert!(!opened.matches("another-state"));
}

#[tokio::test]
async fn a_kept_value_begins_with_its_key_id_and_a_dot() {
    let now = whole_second();
    let kept = sealer_over(&memory_keys(), CLIENT)
        .seal(&pending("/", now), now)
        .await
        .unwrap();
    assert!(kept.expose().starts_with("1."), "{}", kept.expose());
}

#[tokio::test]
async fn a_kept_value_sealed_by_one_sealer_opens_under_another_on_the_same_keys() {
    let now = whole_second();
    let keys = memory_keys();
    let kept = sealer_over(&keys, CLIENT)
        .seal(&pending("/", now), now)
        .await
        .unwrap();
    assert!(sealer_over(&keys, CLIENT).open(&kept, now).await.is_ok());
}

#[tokio::test]
async fn a_kept_value_does_not_open_under_other_keys() {
    let now = whole_second();
    let kept = sealer_over(&memory_keys(), CLIENT)
        .seal(&pending("/", now), now)
        .await
        .unwrap();
    let other = sealer_over(&memory_keys(), CLIENT);
    other.seal(&pending("/", now), now).await.unwrap();
    assert_eq!(
        other.open(&kept, now).await.map(|_| ()),
        Err(SignInRefusal::UnknownState)
    );
}

#[tokio::test]
async fn a_kept_value_with_one_byte_changed_does_not_open_and_the_unchanged_one_does() {
    let now = whole_second();
    let sealer = sealer_over(&memory_keys(), CLIENT);
    let kept = sealer.seal(&pending("/", now), now).await.unwrap();
    let text = kept.expose().to_owned();
    for at in [3, 12, text.len() / 2, text.len() - 2] {
        let mut bytes = text.clone().into_bytes();
        bytes[at] = if bytes[at] == b'A' { b'B' } else { b'A' };
        let changed = KeptState::from_cookie(std::str::from_utf8(&bytes).unwrap());
        assert_eq!(
            sealer.open(&changed, now).await.map(|_| ()),
            Err(SignInRefusal::UnknownState),
            "byte {at}"
        );
    }
    for garbage in ["", "!!!", "AAAA", ".", "0.AAAA", "x.AAAA"] {
        let garbage = KeptState::from_cookie(garbage);
        assert_eq!(
            sealer.open(&garbage, now).await.map(|_| ()),
            Err(SignInRefusal::UnknownState)
        );
    }
    assert!(
        sealer
            .open(&KeptState::from_cookie(&text), now)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_kept_value_does_not_open_for_another_client_id_and_opens_for_its_own() {
    let now = whole_second();
    let keys = memory_keys();
    let kept = sealer_over(&keys, CLIENT)
        .seal(&pending("/", now), now)
        .await
        .unwrap();
    let other = sealer_over(&keys, "another-client");
    assert_eq!(
        other.open(&kept, now).await.map(|_| ()),
        Err(SignInRefusal::UnknownState)
    );
    assert!(sealer_over(&keys, CLIENT).open(&kept, now).await.is_ok());
}

#[tokio::test]
async fn a_kept_value_opens_for_just_under_ten_minutes_and_not_after() {
    let issued = whole_second();
    let sealer = sealer_over(&memory_keys(), CLIENT);
    let kept = sealer.seal(&pending("/", issued), issued).await.unwrap();
    assert!(sealer.open(&kept, at(issued, 599)).await.is_ok());
    assert_eq!(
        sealer.open(&kept, at(issued, 600)).await.map(|_| ()),
        Err(SignInRefusal::UnknownState)
    );
}

#[tokio::test]
async fn a_kept_value_issued_more_than_sixty_seconds_ahead_does_not_open_and_sixty_does() {
    let now = whole_second();
    let sealer = sealer_over(&memory_keys(), CLIENT);
    let ahead = sealer.seal(&pending("/", at(now, 61)), now).await.unwrap();
    assert_eq!(
        sealer.open(&ahead, now).await.map(|_| ()),
        Err(SignInRefusal::UnknownState)
    );
    let just = sealer.seal(&pending("/", at(now, 60)), now).await.unwrap();
    assert!(sealer.open(&just, now).await.is_ok());
}

#[tokio::test]
async fn a_2048_byte_return_path_seals_to_a_cookie_value_under_3100_bytes() {
    let now = whole_second();
    let long = format!("/{}", "x".repeat(2047));
    let sealer = sealer_over(&memory_keys(), CLIENT);
    let kept = sealer.seal(&pending(&long, now), now).await.unwrap();
    assert!(kept.expose().len() < 3100, "{}", kept.expose().len());
    assert!(
        kept.expose()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    );
    assert_eq!(
        sealer
            .open(&kept, now)
            .await
            .unwrap()
            .state
            .expose_secret()
            .len(),
        43
    );
}

#[tokio::test]
async fn a_thousand_seals_use_a_thousand_distinct_random_nonces() {
    let now = whole_second();
    let sealer = sealer_over(&memory_keys(), CLIENT);
    let made = pending("/", now);
    let mut nonces = HashSet::new();
    for _ in 0..1000 {
        let kept = sealer.seal(&made, now).await.unwrap();
        let (_, body) = kept.expose().split_once('.').unwrap();
        nonces.insert(URL_SAFE_NO_PAD.decode(body).unwrap()[..12].to_vec());
    }
    assert_eq!(nonces.len(), 1000);
}

#[tokio::test]
async fn two_sealers_on_one_key_do_not_repeat_a_nonce() {
    let now = whole_second();
    let keys = memory_keys();
    let (first, second) = (sealer_over(&keys, CLIENT), sealer_over(&keys, CLIENT));
    let made = pending("/", now);
    let mut nonces = HashSet::new();
    for sealer in [&first, &second] {
        for _ in 0..200 {
            let kept = sealer.seal(&made, now).await.unwrap();
            let (_, body) = kept.expose().split_once('.').unwrap();
            nonces.insert(URL_SAFE_NO_PAD.decode(body).unwrap()[..12].to_vec());
        }
    }
    assert_eq!(nonces.len(), 400);
}

#[tokio::test]
async fn after_a_rotation_new_values_use_the_new_key_and_another_sealer_opens_them() {
    let start = whole_second();
    let keys = memory_keys();
    let sealer = sealer_over(&keys, CLIENT);
    let old = sealer.seal(&pending("/", start), start).await.unwrap();
    assert!(old.expose().starts_with("1."));
    let later = at(start, 30 * DAY);
    let fresh = sealer.seal(&pending("/", later), later).await.unwrap();
    assert!(fresh.expose().starts_with("2."), "{}", fresh.expose());
    assert!(sealer_over(&keys, CLIENT).open(&fresh, later).await.is_ok());
}

#[tokio::test]
async fn the_old_key_still_opens_a_value_an_hour_after_rotation_and_not_later() {
    let start = whole_second();
    let keys = Arc::new(Counting::default());
    let dynamic: Arc<dyn DynKeys> = keys.clone();
    let sealer = sealer_over(&dynamic, CLIENT);
    sealer.seal(&pending("/", start), start).await.unwrap();
    let rotated = at(start, 30 * DAY);
    sealer.seal(&pending("/", rotated), rotated).await.unwrap();
    let stamp = |time: SystemTime| Timestamp::from_secs(unix_seconds(time));
    assert!(
        SealingKeys::get(&*keys, KeyId::FIRST, stamp(at(rotated, 3599)))
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        SealingKeys::get(&*keys, KeyId::FIRST, stamp(at(rotated, 3600)))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_value_naming_an_unknown_key_makes_one_store_read_before_it_is_refused() {
    let now = whole_second();
    let keys = Arc::new(Counting::default());
    let dynamic: Arc<dyn DynKeys> = keys.clone();
    let sealer = sealer_over(&dynamic, CLIENT);
    sealer.seal(&pending("/", now), now).await.unwrap();
    let before = keys.gets.load(Ordering::Relaxed);
    let unknown = KeptState::from_cookie("9.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    assert_eq!(
        sealer.open(&unknown, now).await.map(|_| ()),
        Err(SignInRefusal::UnknownState)
    );
    assert_eq!(keys.gets.load(Ordering::Relaxed), before + 1);
}

#[tokio::test]
async fn a_sealer_holds_a_key_for_five_minutes_before_it_reads_the_store_again() {
    let now = whole_second();
    let keys = Arc::new(Counting::default());
    let dynamic: Arc<dyn DynKeys> = keys.clone();
    let sealer = sealer_over(&dynamic, CLIENT);
    let kept = sealer.seal(&pending("/", now), now).await.unwrap();
    for _ in 0..5 {
        sealer.open(&kept, now).await.unwrap();
    }
    assert_eq!(keys.gets.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn an_unreachable_key_store_refuses_a_start_and_an_open_as_store_unavailable() {
    let now = whole_second();
    let down: Arc<dyn DynKeys> = Arc::new(Down);
    let sealer = sealer_over(&down, CLIENT);
    assert_eq!(
        sealer.seal(&pending("/", now), now).await.map(|_| ()),
        Err(SignInRefusal::StoreUnavailable)
    );
    let kept = KeptState::from_cookie("1.AAAA");
    assert_eq!(
        sealer.open(&kept, now).await.map(|_| ()),
        Err(SignInRefusal::StoreUnavailable)
    );
}

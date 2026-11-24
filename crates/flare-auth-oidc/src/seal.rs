//! The sealer: pending sign-ins sealed into kept values with AES-256-GCM
//! under a key from a [`SealingKeys`](flare_core::SealingKeys) store.
//!
//! A kept value is `<key id>.<base64url of a 12-byte random nonce and the
//! ciphertext>`. The key id is not secret. The sealer holds each key it
//! reads for 5 minutes. A value that names an id the sealer does not hold
//! makes one read of the store before the value is refused as
//! [`SignInRefusal::UnknownState`]. Each seal draws a new 12-byte nonce
//! from the system's random number generator, so two processes that share
//! a key do not repeat a nonce except with the probability of a 96-bit
//! collision over the sign-ins one 30-day key sees.

use crate::clock::unix_seconds;
use crate::config::ClientId;
use crate::flow::{KeptState, Pending, ReturnTo};
use crate::refusal::SignInRefusal;
use crate::stores::DynKeys;
use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use aws_lc_rs::rand;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use flare_core::{KeyBytes, KeyId, SealingKey, Timestamp};
use secrecy::zeroize::Zeroize as _;
use secrecy::{ExposeSecret as _, SecretString};
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

/// How long a pending sign-in stays usable, in seconds.
const PENDING_SECONDS: u64 = 10 * 60;
/// How far ahead of the clock a sealed issue time may lie, in seconds.
const AHEAD_SECONDS: u64 = 60;
/// The length of a random value in base64url: 32 bytes, unpadded.
pub(crate) const RANDOM_LEN: usize = 43;
/// The sealed layout: the issue time, then the state, nonce and verifier,
/// then the return-to value.
const FIXED_LEN: usize = 8 + 3 * RANDOM_LEN;
/// The first part of the associated data; the client ID follows it.
const AAD_LABEL: &[u8] = b"flare-oidc-kept-v2\0";
/// How long the sealer holds a key it read.
const KEEP: Duration = Duration::from_secs(5 * 60);

/// A key and when it was read.
struct Held {
    key: SealingKey,
    read: SystemTime,
}

#[derive(Default)]
struct Cache {
    current: Option<Held>,
    older: Vec<Held>,
}

/// Seals pending sign-ins into kept values and opens them again.
pub(crate) struct Sealer {
    keys: Arc<dyn DynKeys>,
    cache: Mutex<Cache>,
    aad: Vec<u8>,
}

/// 32 fresh random bytes for a store call that may write a key.
fn fresh() -> Result<KeyBytes, SignInRefusal> {
    let mut bytes = [0_u8; 32];
    rand::fill(&mut bytes).map_err(|_| SignInRefusal::NoRandomness)?;
    let key = KeyBytes::new(bytes);
    bytes.zeroize();
    Ok(key)
}

fn stamp(now: SystemTime) -> Timestamp {
    Timestamp::from_secs(unix_seconds(now))
}

fn aead(key: &SealingKey) -> Option<LessSafeKey> {
    UnboundKey::new(&AES_256_GCM, key.bytes.expose())
        .ok()
        .map(LessSafeKey::new)
}

fn is_fresh(held: &Held, now: SystemTime) -> bool {
    now.duration_since(held.read).map_or(true, |age| age < KEEP)
}

impl Sealer {
    /// A sealer over `keys` for the client `client`.
    pub(crate) fn new(keys: Arc<dyn DynKeys>, client: &ClientId) -> Self {
        let mut aad = AAD_LABEL.to_vec();
        aad.extend_from_slice(client.as_str().as_bytes());
        Self {
            keys,
            cache: Mutex::new(Cache::default()),
            aad,
        }
    }

    fn cached(
        &self,
        pick: impl FnOnce(&Cache) -> Option<&Held>,
        now: SystemTime,
    ) -> Option<SealingKey> {
        let cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        pick(&cache)
            .filter(|held| is_fresh(held, now))
            .map(|held| held.key.clone())
    }

    fn keep(&self, key: &SealingKey, now: SystemTime, as_current: bool) {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        let held = Held {
            key: key.clone(),
            read: now,
        };
        if as_current {
            if let Some(previous) = cache.current.take() {
                cache.older.push(previous);
            }
            cache.current = Some(held);
        } else {
            cache
                .older
                .retain(|old| old.key.id != key.id && is_fresh(old, now));
            cache.older.push(held);
        }
    }

    /// The newest key, from the cache when it was read less than 5 minutes
    /// ago. A key 30 days old or more is rotated first.
    async fn current(&self, now: SystemTime) -> Result<SealingKey, SignInRefusal> {
        let stamped = stamp(now);
        if let Some(key) = self.cached(|cache| cache.current.as_ref(), now)
            && !key.is_due(stamped)
        {
            return Ok(key);
        }
        let unavailable = |_| SignInRefusal::StoreUnavailable;
        let mut key = self
            .keys
            .current(fresh()?, stamped)
            .await
            .map_err(unavailable)?;
        if key.is_due(stamped) {
            key = self
                .keys
                .rotate(fresh()?, stamped)
                .await
                .map_err(unavailable)?;
        }
        self.keep(&key, now, true);
        Ok(key)
    }

    /// The key `id`: the cached one, or one read of the store.
    async fn named(&self, id: KeyId, now: SystemTime) -> Result<SealingKey, SignInRefusal> {
        let cached = self
            .cached(|cache| cache.current.as_ref(), now)
            .filter(|key| key.id == id);
        if let Some(key) = cached {
            return Ok(key);
        }
        let older = self.cached(
            |cache| cache.older.iter().find(|held| held.key.id == id),
            now,
        );
        if let Some(key) = older {
            return Ok(key);
        }
        let read = self
            .keys
            .get(id, stamp(now))
            .await
            .map_err(|_| SignInRefusal::StoreUnavailable)?;
        let key = read.ok_or(SignInRefusal::UnknownState)?;
        self.keep(&key, now, false);
        Ok(key)
    }

    /// Seals `pending` into a kept value.
    pub(crate) async fn seal(
        &self,
        pending: &Pending,
        now: SystemTime,
    ) -> Result<KeptState, SignInRefusal> {
        let key = self.current(now).await?;
        let aead = aead(&key).ok_or(SignInRefusal::CannotSeal)?;
        let mut nonce = [0_u8; NONCE_LEN];
        rand::fill(&mut nonce).map_err(|_| SignInRefusal::NoRandomness)?;
        let mut sealed = Vec::with_capacity(FIXED_LEN + pending.return_to.as_str().len() + 16);
        sealed.extend_from_slice(&pending.issued().to_be_bytes());
        for value in [&pending.state, &pending.nonce, &pending.verifier] {
            sealed.extend_from_slice(value.expose_secret().as_bytes());
        }
        sealed.extend_from_slice(pending.return_to.as_str().as_bytes());
        let aad = Aad::from(self.aad.as_slice());
        if aead
            .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), aad, &mut sealed)
            .is_err()
        {
            sealed.zeroize();
            return Err(SignInRefusal::CannotSeal);
        }
        let mut value = nonce.to_vec();
        value.extend_from_slice(&sealed);
        let text = format!("{}.{}", key.id.get(), URL_SAFE_NO_PAD.encode(value));
        Ok(KeptState::from_sealed(text))
    }

    /// Opens a kept value. Anything that does not open under the key it
    /// names, names no readable key, was issued 10 minutes or more before
    /// `now`, or was issued more than 60 seconds after `now`, is
    /// [`SignInRefusal::UnknownState`].
    pub(crate) async fn open(
        &self,
        kept: &KeptState,
        now: SystemTime,
    ) -> Result<Pending, SignInRefusal> {
        let unknown = SignInRefusal::UnknownState;
        let (id, body) = kept.expose().split_once('.').ok_or(unknown)?;
        let id = id.parse().ok().and_then(KeyId::new).ok_or(unknown)?;
        let mut bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| unknown)?;
        let key = self.named(id, now).await;
        let opened = key.map(|key| self.open_bytes(&key, &mut bytes, now));
        bytes.zeroize();
        opened?.ok_or(unknown)
    }

    fn open_bytes(&self, key: &SealingKey, bytes: &mut [u8], now: SystemTime) -> Option<Pending> {
        let (nonce, sealed) = bytes.split_at_mut_checked(NONCE_LEN)?;
        let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
        let aad = Aad::from(self.aad.as_slice());
        let plain = aead(key)?.open_in_place(nonce, aad, sealed).ok()?;
        let (issued, rest) = plain.split_first_chunk::<8>()?;
        let issued = u64::from_be_bytes(*issued);
        let now = unix_seconds(now);
        let expired = now >= issued.checked_add(PENDING_SECONDS)?;
        let ahead = issued > now.checked_add(AHEAD_SECONDS)?;
        if expired || ahead {
            return None;
        }
        let text = std::str::from_utf8(rest).ok()?;
        let (state, rest) = text.split_at_checked(RANDOM_LEN)?;
        let (nonce, rest) = rest.split_at_checked(RANDOM_LEN)?;
        let (verifier, return_to) = rest.split_at_checked(RANDOM_LEN)?;
        Some(Pending::from_parts(
            issued,
            SecretString::from(state),
            SecretString::from(nonce),
            SecretString::from(verifier),
            ReturnTo::try_from(return_to).ok()?,
        ))
    }
}

impl fmt::Debug for Sealer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Sealer").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;

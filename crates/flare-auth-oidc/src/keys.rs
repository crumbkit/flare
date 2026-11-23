//! The provider's key set: which keys are usable, and the cache that holds
//! the last checked set.

use crate::clock::has_passed;
use crate::provider::Provider;
use jsonwebtoken::DecodingKey;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

/// How long a fetched key set is used before it is fetched again.
const KEY_SET_LIFETIME: Duration = Duration::from_secs(60 * 60);
/// The shortest time between two fetches for an unknown key id.
const REFRESH_INTERVAL: Duration = Duration::from_secs(60);

/// One key of a JSON Web Key Set, with the fields the crate reads.
#[derive(Deserialize)]
struct Jwk {
    kty: Option<String>,
    #[serde(rename = "use")]
    usage: Option<String>,
    alg: Option<String>,
    kid: Option<String>,
    n: Option<String>,
    e: Option<String>,
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

/// The usable keys of a key set, by key id. It always holds at least one
/// key.
#[derive(Clone)]
pub(crate) struct KeySet(HashMap<String, DecodingKey>);

impl KeySet {
    /// Reads a key set and keeps each RSA key whose `use` is absent or
    /// `sig`, whose `alg` is absent or RS256, and which has a key id.
    /// Returns `None` when no key is usable.
    pub(crate) fn parse(body: &[u8]) -> Option<Self> {
        let jwks: Jwks = serde_json::from_slice(body).ok()?;
        let keys: HashMap<String, DecodingKey> = jwks
            .keys
            .into_iter()
            .filter(|key| key.kty.as_deref() == Some("RSA"))
            .filter(|key| key.usage.as_deref().is_none_or(|usage| usage == "sig"))
            .filter(|key| key.alg.as_deref().is_none_or(|alg| alg == "RS256"))
            .filter_map(|key| {
                let decoding = DecodingKey::from_rsa_components(&key.n?, &key.e?).ok()?;
                Some((key.kid?, decoding))
            })
            .collect();
        (!keys.is_empty()).then_some(Self(keys))
    }
}

/// Why no key was found for a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyError {
    /// The key set, fetched again or not, has no key with that id.
    Unknown,
    /// The key set could not be fetched.
    Unavailable,
    /// The fetched key set held no usable key.
    Unusable,
}

struct Cached {
    keys: KeySet,
    fetched: SystemTime,
    /// When a sign-in last fetched the set again. The fetch at `connect`
    /// does not count, so the first unknown key id always causes a fetch.
    last_attempt: Option<SystemTime>,
}

/// The last checked key set. A fetched set replaces it only once it is
/// checked, so the cache always holds a usable set.
///
/// One sign-in at a time fetches the set again. A sign-in that needs a fetch
/// while another one is fetching waits for that fetch and then looks its key
/// id up again, so two sign-ins that present a newly rotated key together
/// both find it after one fetch.
pub(crate) struct KeyCache {
    cached: Mutex<Cached>,
    refreshing: tokio::sync::Mutex<()>,
}

/// What a lookup in the cache found.
enum Lookup {
    /// The key, in a set under one hour old.
    Found(DecodingKey),
    /// The cache must fetch the set again first.
    Refresh,
    /// The key id is unknown, and the last fetch was under 60 seconds ago.
    Unknown,
}

impl KeyCache {
    /// A cache holding `keys`, fetched at `now`.
    pub(crate) fn new(keys: KeySet, now: SystemTime) -> Self {
        Self {
            cached: Mutex::new(Cached {
                keys,
                fetched: now,
                last_attempt: None,
            }),
            refreshing: tokio::sync::Mutex::new(()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Cached> {
        self.cached.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn look_up(&self, kid: &str, now: SystemTime) -> Lookup {
        let cached = self.lock();
        let stale = has_passed(cached.fetched, KEY_SET_LIFETIME, now);
        match cached.keys.0.get(kid) {
            Some(key) if !stale => Lookup::Found(key.clone()),
            _ if stale => Lookup::Refresh,
            _ if cached
                .last_attempt
                .is_none_or(|last| has_passed(last, REFRESH_INTERVAL, now)) =>
            {
                Lookup::Refresh
            }
            _ => Lookup::Unknown,
        }
    }

    /// The key for `kid`. A key set older than one hour is fetched again
    /// first. An unknown `kid` causes one fetch, at most once every 60
    /// seconds.
    pub(crate) async fn key_for(
        &self,
        kid: &str,
        provider: &Provider,
        now: SystemTime,
    ) -> Result<DecodingKey, KeyError> {
        if let Lookup::Found(key) = self.look_up(kid, now) {
            return Ok(key);
        }
        let _refreshing = self.refreshing.lock().await;
        match self.look_up(kid, now) {
            Lookup::Found(key) => return Ok(key),
            Lookup::Unknown => return Err(KeyError::Unknown),
            Lookup::Refresh => self.lock().last_attempt = Some(now),
        }
        let body = provider
            .fetch_keys()
            .await
            .map_err(|_| KeyError::Unavailable)?;
        let keys = KeySet::parse(&body).ok_or(KeyError::Unusable)?;
        let found = keys.0.get(kid).cloned();
        {
            let mut cached = self.lock();
            cached.keys = keys;
            cached.fetched = now;
        }
        found.ok_or(KeyError::Unknown)
    }
}

impl std::fmt::Debug for KeyCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cached = self.lock();
        formatter
            .debug_struct("KeyCache")
            .field("keys", &cached.keys.0.len())
            .finish_non_exhaustive()
    }
}

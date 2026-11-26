//! The provider's key set: which keys are usable, and the cache that holds
//! the last checked set. The OpenID Connect sign-in keeps RSA keys for
//! RS256; the proxy sign-in also keeps P-256 keys for ES256, as its
//! [`KeyRule`] says.

use crate::clock::has_passed;
use crate::provider::{Provider, ReplyError};
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
    crv: Option<String>,
    x: Option<String>,
    y: Option<String>,
}

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

/// Which keys of a key set are usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyRule {
    /// RSA keys, for RS256.
    pub(crate) rs256: bool,
    /// P-256 keys, for ES256.
    pub(crate) es256: bool,
    /// The most keys a set may hold, usable or not, or `None` for no limit.
    pub(crate) max_keys: Option<usize>,
}

impl KeyRule {
    /// The OpenID Connect sign-in's rule: RSA keys for RS256, any number.
    pub(crate) const OIDC: Self = Self {
        rs256: true,
        es256: false,
        max_keys: None,
    };

    /// The decoding key of `key`, when this rule admits it. A key is usable
    /// when its `use` is absent or `sig`, it has a key id, and it is an RSA
    /// key whose `alg` is absent or RS256, or a P-256 key whose `alg` is
    /// absent or ES256, as the rule allows.
    fn admit(self, key: Jwk) -> Option<(String, DecodingKey)> {
        if key.usage.as_deref().is_some_and(|usage| usage != "sig") {
            return None;
        }
        let alg = key.alg.as_deref();
        let decoding = match key.kty.as_deref() {
            Some("RSA") if self.rs256 && alg.is_none_or(|alg| alg == "RS256") => {
                DecodingKey::from_rsa_components(&key.n?, &key.e?).ok()?
            }
            Some("EC")
                if self.es256
                    && key.crv.as_deref() == Some("P-256")
                    && alg.is_none_or(|alg| alg == "ES256") =>
            {
                DecodingKey::from_ec_components(&key.x?, &key.y?).ok()?
            }
            _ => return None,
        };
        Some((key.kid?, decoding))
    }
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
        Self::parse_under(body, KeyRule::OIDC)
    }

    /// Reads a key set and keeps the keys that `rule` admits. Returns
    /// `None` when no key is usable or the set holds more keys than the
    /// rule allows.
    pub(crate) fn parse_under(body: &[u8], rule: KeyRule) -> Option<Self> {
        let jwks: Jwks = serde_json::from_slice(body).ok()?;
        if rule.max_keys.is_some_and(|max| jwks.keys.len() > max) {
            return None;
        }
        let keys: HashMap<String, DecodingKey> = jwks
            .keys
            .into_iter()
            .filter_map(|key| rule.admit(key))
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
    /// When a sign-in's fetch of the set last completed, whether it
    /// succeeded or failed. The fetch at `connect` does not count, so the
    /// first unknown key id always causes a fetch. A fetch whose sign-in is
    /// dropped before it completes does not set it.
    last_attempt: Option<SystemTime>,
    /// How many sign-in fetches have completed. A sign-in that waited for
    /// another one's fetch compares it with the count it saw before waiting.
    completed: u64,
    /// Why the last completed fetch failed, or `None` when it succeeded.
    failure: Option<KeyError>,
}

impl Cached {
    /// The key for `kid` after the last completed fetch: that fetch's
    /// failure, or the key from the set it cached.
    fn outcome(&self, kid: &str) -> Result<DecodingKey, KeyError> {
        match self.failure {
            Some(failure) => Err(failure),
            None => self.keys.0.get(kid).cloned().ok_or(KeyError::Unknown),
        }
    }
}

/// The last checked key set. A fetched set replaces it only once it is
/// checked, so the cache always holds a usable set.
///
/// One sign-in at a time fetches the set again. A sign-in that needs a fetch
/// while another one is fetching waits for that fetch and takes its outcome:
/// on success it looks its key id up in the new set, and on failure it gets
/// the same failure. It starts no fetch of its own, so sign-ins that arrive
/// together cost one fetch and one request deadline, whether the provider
/// answers or not. When the fetching sign-in is dropped before its fetch
/// completes, the next waiter fetches in its place.
pub(crate) struct KeyCache {
    cached: Mutex<Cached>,
    refreshing: tokio::sync::Mutex<()>,
    rule: KeyRule,
}

/// What a lookup in the cache found.
enum Lookup {
    /// The key, in a set under one hour old.
    Found(DecodingKey),
    /// The cache must fetch the set again first. It holds how many fetches
    /// had completed when the cache was read.
    Refresh(u64),
    /// The key id is unknown, and the last fetch was under 60 seconds ago.
    Unknown,
}

impl KeyCache {
    /// A cache holding `keys`, fetched at `now`, that keeps the keys of a
    /// later fetch under [`KeyRule::OIDC`].
    pub(crate) fn new(keys: KeySet, now: SystemTime) -> Self {
        Self::under(keys, now, KeyRule::OIDC)
    }

    /// A cache holding `keys`, fetched at `now`, that keeps the keys of a
    /// later fetch under `rule`.
    pub(crate) fn under(keys: KeySet, now: SystemTime, rule: KeyRule) -> Self {
        Self {
            cached: Mutex::new(Cached {
                keys,
                fetched: now,
                last_attempt: None,
                completed: 0,
                failure: None,
            }),
            refreshing: tokio::sync::Mutex::new(()),
            rule,
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
            _ if stale => Lookup::Refresh(cached.completed),
            _ if cached
                .last_attempt
                .is_none_or(|last| has_passed(last, REFRESH_INTERVAL, now)) =>
            {
                Lookup::Refresh(cached.completed)
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
        self.key_with(kid, now, || provider.fetch_keys()).await
    }

    /// [`KeyCache::key_for`], with `fetch` in place of the provider's key
    /// set endpoint, as the proxy sign-in fetches its configured address.
    pub(crate) async fn key_with<F>(
        &self,
        kid: &str,
        now: SystemTime,
        fetch: impl FnOnce() -> F,
    ) -> Result<DecodingKey, KeyError>
    where
        F: Future<Output = Result<Vec<u8>, ReplyError>>,
    {
        let seen = match self.look_up(kid, now) {
            Lookup::Found(key) => return Ok(key),
            Lookup::Unknown => return Err(KeyError::Unknown),
            Lookup::Refresh(completed) => completed,
        };
        let _refreshing = self.refreshing.lock().await;
        {
            let cached = self.lock();
            if cached.completed != seen {
                // Another sign-in's fetch completed while this one waited,
                // so this one takes that fetch's outcome.
                return cached.outcome(kid);
            }
        }
        let fetched = match fetch().await {
            Ok(body) => KeySet::parse_under(&body, self.rule).ok_or(KeyError::Unusable),
            Err(_) => Err(KeyError::Unavailable),
        };
        // The outcome is stored before `_refreshing` is released, so each
        // waiter finds it.
        let mut cached = self.lock();
        cached.last_attempt = Some(now);
        cached.completed += 1;
        match fetched {
            Ok(keys) => {
                cached.keys = keys;
                cached.fetched = now;
                cached.failure = None;
            }
            Err(failure) => cached.failure = Some(failure),
        }
        cached.outcome(kid)
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

#[cfg(test)]
mod tests;

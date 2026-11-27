//! The content host: the second hostname of a hosted site, which serves the
//! HTML file of an uploaded HTML share to a sandboxed frame and nothing else.
//!
//! A browser gives each origin its own storage and cookies, so an uploaded
//! page that runs its own scripts reaches nothing of flare's when it is
//! served from another origin. The content host answers only `GET /v/<token>`
//! and `/up`, reads no cookie and sets none. The reading page of a share at
//! `/s/<id>` mints a view token for the signed-in reader and frames
//! `https://<content host>/v/<token>`. `token.rs` seals and opens the token,
//! and `serve.rs` answers the request in this order:
//!
//! 1. The Host header names the content host, which the caller checked, and
//!    the path is `/v/<token>`, or else 404.
//! 2. `Sec-Fetch-Dest: iframe`, or else 403 `content::not_framed`, so the
//!    address does not open as a page of its own.
//! 3. The token opens under the sealing key it names, or else 404.
//! 4. The token has not expired, or else 410.
//! 5. The viewer may still read the share, the version exists and the file
//!    is stored, or else 404, which is the same answer as step 1 and 3 give.
//!    A store failure answers 503 `server::busy`.
//!
//! A deployment that sets no content host refuses HTML shares when they are
//! uploaded, and answers 403 `request::host` for the content host's name.

mod serve;
#[cfg(test)]
mod tests;
mod token;

pub(crate) use serve::serve;
pub(crate) use token::Claims;

use crate::site::PublicOrigin;
use aws_lc_rs::rand;
use flare_core::{KeyBytes, KeyId, SealingKey, SealingKeys, SealingKeysError, Timestamp};
use secrecy::zeroize::Zeroize as _;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use token::Refused;

/// A boxed future that borrows its store.
type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// [`SealingKeys`] as a trait object, so the app keeps its type parameters.
trait ViewKeys: Send + Sync {
    fn current<'a>(
        &'a self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Boxed<'a, Result<SealingKey, SealingKeysError>>;

    fn get<'a>(
        &'a self,
        id: KeyId,
        now: Timestamp,
    ) -> Boxed<'a, Result<Option<SealingKey>, SealingKeysError>>;

    fn rotate<'a>(
        &'a self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Boxed<'a, Result<SealingKey, SealingKeysError>>;
}

impl<T: SealingKeys> ViewKeys for T {
    fn current<'a>(
        &'a self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Boxed<'a, Result<SealingKey, SealingKeysError>> {
        Box::pin(SealingKeys::current(self, fresh, now))
    }

    fn get<'a>(
        &'a self,
        id: KeyId,
        now: Timestamp,
    ) -> Boxed<'a, Result<Option<SealingKey>, SealingKeysError>> {
        Box::pin(SealingKeys::get(self, id, now))
    }

    fn rotate<'a>(
        &'a self,
        fresh: KeyBytes,
        now: Timestamp,
    ) -> Boxed<'a, Result<SealingKey, SealingKeysError>> {
        Box::pin(SealingKeys::rotate(self, fresh, now))
    }
}

/// Why a view token could not be minted or opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewFailure {
    /// The token does not open: it is malformed, names a key that is no
    /// longer readable, or fails its seal.
    Unknown,
    /// The token's 10 minutes have passed.
    Expired,
    /// The key store did not answer, or the system has no randomness.
    Unavailable,
}

/// The second hostname of a hosted site and the keys that seal its view
/// tokens.
///
/// Shortcut: each mint and each open reads the key store, with no cache of
/// the key, so a view costs one extra read of a small item. A cache with a
/// few minutes of life, as the sign-in's sealer keeps, would remove the
/// read once page views make it matter.
#[derive(Clone)]
pub struct ContentHost {
    origin: PublicOrigin,
    keys: Arc<dyn ViewKeys>,
}

impl ContentHost {
    /// The content host at `origin`, whose view tokens are sealed under keys
    /// from `keys`. `origin` must differ from the site's own origin in its
    /// host, so a browser gives the two separate storage.
    #[must_use]
    pub fn new(origin: PublicOrigin, keys: impl SealingKeys + 'static) -> Self {
        Self {
            origin,
            keys: Arc::new(keys),
        }
    }

    /// The content host's origin.
    #[must_use]
    pub fn origin(&self) -> &PublicOrigin {
        &self.origin
    }

    /// A fresh 32 bytes for a store call that may write a key.
    fn fresh() -> Result<KeyBytes, ViewFailure> {
        let mut bytes = [0_u8; 32];
        rand::fill(&mut bytes).map_err(|_| ViewFailure::Unavailable)?;
        let key = KeyBytes::new(bytes);
        bytes.zeroize();
        Ok(key)
    }

    /// The text of a view token for `claims`, issued at `now` seconds since
    /// 1970. A key 30 days old or more is rotated first.
    pub(crate) async fn mint(&self, claims: &Claims, now: u64) -> Result<String, ViewFailure> {
        let stamped = Timestamp::from_secs(now);
        let unavailable = |_| ViewFailure::Unavailable;
        let mut key = self
            .keys
            .current(Self::fresh()?, stamped)
            .await
            .map_err(unavailable)?;
        if key.is_due(stamped) {
            key = self
                .keys
                .rotate(Self::fresh()?, stamped)
                .await
                .map_err(unavailable)?;
        }
        token::seal(&key, claims, now).ok_or(ViewFailure::Unavailable)
    }

    /// What the view token `text` says, at `now` seconds since 1970.
    pub(crate) async fn open(&self, text: &str, now: u64) -> Result<Claims, ViewFailure> {
        let (id, body) = token::split(text).ok_or(ViewFailure::Unknown)?;
        let id = KeyId::new(id).ok_or(ViewFailure::Unknown)?;
        let key = self
            .keys
            .get(id, Timestamp::from_secs(now))
            .await
            .map_err(|_| ViewFailure::Unavailable)?
            .ok_or(ViewFailure::Unknown)?;
        token::open(&key, body, now).map_err(|refused| match refused {
            Refused::Invalid => ViewFailure::Unknown,
            Refused::Expired => ViewFailure::Expired,
        })
    }
}

impl fmt::Debug for ContentHost {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ContentHost")
            .field("origin", &self.origin)
            .finish_non_exhaustive()
    }
}

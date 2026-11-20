//! Pending sign-ins: the values that `start` makes, the sealed kept state
//! that carries them to the callback, and the callback query parser.
//!
//! A pending sign-in lives only in the browser's kept-state cookie, sealed
//! with AES-256-GCM under a key that exists only in this process's memory,
//! so no `start` stores anything server-side.

use crate::clock::unix_seconds;
use crate::config::{ClientId, ConfigError, OidcConfig};
use crate::refusal::SignInRefusal;
use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use aws_lc_rs::{constant_time, digest, rand};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use secrecy::zeroize::Zeroize as _;
use secrecy::{ExposeSecret as _, SecretString};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;
use url::Url;

/// How long a pending sign-in stays usable, in seconds.
const PENDING_SECONDS: u64 = 10 * 60;
/// How far ahead of the clock a sealed issue time may lie, in seconds.
const AHEAD_SECONDS: u64 = 60;
/// The longest callback query that `complete` reads.
const MAX_QUERY: usize = 4096;
/// The longest return-to value.
const MAX_RETURN_TO: usize = 2048;
/// The length of a random value in base64url: 32 bytes, unpadded.
const RANDOM_LEN: usize = 43;
/// The sealed layout: the issue time, then the state, nonce and verifier,
/// then the return-to value.
const FIXED_LEN: usize = 8 + 3 * RANDOM_LEN;
/// The first part of the associated data; the client ID follows it.
const AAD_LABEL: &[u8] = b"hangar-oidc-kept-v1\0";

/// 32 random bytes in base64url, as a secret.
pub(crate) fn random_secret() -> Result<SecretString, SignInRefusal> {
    let mut bytes = [0_u8; 32];
    rand::fill(&mut bytes).map_err(|_| SignInRefusal::NoRandomness)?;
    Ok(SecretString::from(URL_SAFE_NO_PAD.encode(bytes)))
}

/// The SHA-256 of `text`, which the session table uses as a key so it
/// never holds the session values themselves.
pub(crate) fn hash(text: &str) -> [u8; 32] {
    let mut key = [0_u8; 32];
    key.copy_from_slice(digest::digest(&digest::SHA256, text.as_bytes()).as_ref());
    key
}

/// The S256 PKCE challenge of `verifier` (RFC 7636 section 4.2).
pub(crate) fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, verifier.as_bytes()))
}

/// Where the server sends the person after the sign-in. The crate seals
/// it at `start` and returns it unread from `complete`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnTo(String);

impl TryFrom<&str> for ReturnTo {
    type Error = ConfigError;

    /// Admits at most 2,048 bytes.
    fn try_from(text: &str) -> Result<Self, ConfigError> {
        if text.len() > MAX_RETURN_TO {
            return Err(ConfigError::ReturnTo);
        }
        Ok(Self(text.to_owned()))
    }
}

impl ReturnTo {
    /// The value the server gave.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The sealed pending sign-in that the server keeps in a cookie between
/// `start` and `complete`: base64url of a 12-byte nonce and the
/// AES-256-GCM ciphertext. Its `Debug` output hides the value.
#[derive(Debug, Clone)]
pub struct KeptState(SecretString);

impl KeptState {
    /// The value as the browser sent it back in the cookie.
    #[must_use]
    pub fn from_cookie(value: &str) -> Self {
        Self(SecretString::from(value))
    }

    /// The value to put in the cookie.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }
}

/// What `start` returns: the provider URL to redirect the browser to, and
/// the sealed state for the server to keep in a cookie.
#[non_exhaustive]
pub struct SignInStart {
    redirect: Url,
    kept: KeptState,
}

impl SignInStart {
    /// The provider URL to redirect the browser to.
    #[must_use]
    pub fn redirect(&self) -> &Url {
        &self.redirect
    }

    /// The sealed state to keep in a cookie until the callback.
    #[must_use]
    pub fn kept(&self) -> &KeptState {
        &self.kept
    }
}

impl fmt::Debug for SignInStart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut shown = self.redirect.clone();
        shown.set_query(None);
        formatter
            .debug_struct("SignInStart")
            .field("redirect", &shown.as_str())
            .field("kept", &self.kept)
            .finish()
    }
}

/// A sign-in between `start` and the callback.
#[derive(Debug)]
pub(crate) struct Pending {
    issued: u64,
    pub(crate) state: SecretString,
    pub(crate) nonce: SecretString,
    pub(crate) verifier: SecretString,
    pub(crate) return_to: ReturnTo,
}

impl Pending {
    /// A new sign-in issued at `now`, with a random state, nonce and PKCE
    /// verifier.
    pub(crate) fn new(return_to: ReturnTo, now: SystemTime) -> Result<Self, SignInRefusal> {
        Ok(Self {
            issued: unix_seconds(now),
            state: random_secret()?,
            nonce: random_secret()?,
            verifier: random_secret()?,
            return_to,
        })
    }

    /// Whether `state` equals the sealed state, compared in constant time.
    pub(crate) fn matches(&self, state: &str) -> bool {
        constant_time::verify_slices_are_equal(
            self.state.expose_secret().as_bytes(),
            state.as_bytes(),
        )
        .is_ok()
    }
}

/// Seals pending sign-ins into kept values and opens them again, under one
/// AES-256-GCM key drawn when the authenticator connects.
///
/// Each nonce is four zero bytes and the next value of a counter that
/// starts at zero. The key exists only in this process and is never
/// reused by another, so the counter never repeats a nonce under it. A
/// counter that would pass `u64::MAX` refuses to seal rather than wrap.
pub(crate) struct Sealer {
    key: LessSafeKey,
    counter: AtomicU64,
    aad: Vec<u8>,
}

impl Sealer {
    /// A sealer with a new random key, for the client `client`.
    pub(crate) fn new(client: &ClientId) -> Option<Self> {
        let mut bytes = [0_u8; 32];
        rand::fill(&mut bytes).ok()?;
        let sealer = Self::from_bytes(&bytes, client);
        bytes.zeroize();
        sealer
    }

    fn from_bytes(bytes: &[u8; 32], client: &ClientId) -> Option<Self> {
        let key = UnboundKey::new(&AES_256_GCM, bytes).ok()?;
        let mut aad = AAD_LABEL.to_vec();
        aad.extend_from_slice(client.as_str().as_bytes());
        Some(Self {
            key: LessSafeKey::new(key),
            counter: AtomicU64::new(0),
            aad,
        })
    }

    fn next_nonce(&self) -> Result<[u8; NONCE_LEN], SignInRefusal> {
        let count = self
            .counter
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                count.checked_add(1)
            })
            .map_err(|_| SignInRefusal::CannotSeal)?;
        let mut nonce = [0_u8; NONCE_LEN];
        for (slot, byte) in nonce.iter_mut().skip(4).zip(count.to_be_bytes()) {
            *slot = byte;
        }
        Ok(nonce)
    }

    /// Seals `pending` into a kept value.
    pub(crate) fn seal(&self, pending: &Pending) -> Result<KeptState, SignInRefusal> {
        let nonce = self.next_nonce()?;
        let mut sealed = Vec::with_capacity(FIXED_LEN + pending.return_to.as_str().len() + 16);
        sealed.extend_from_slice(&pending.issued.to_be_bytes());
        for value in [&pending.state, &pending.nonce, &pending.verifier] {
            sealed.extend_from_slice(value.expose_secret().as_bytes());
        }
        sealed.extend_from_slice(pending.return_to.as_str().as_bytes());
        let aad = Aad::from(self.aad.as_slice());
        if self
            .key
            .seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), aad, &mut sealed)
            .is_err()
        {
            sealed.zeroize();
            return Err(SignInRefusal::CannotSeal);
        }
        let mut value = nonce.to_vec();
        value.extend_from_slice(&sealed);
        Ok(KeptState(SecretString::from(URL_SAFE_NO_PAD.encode(value))))
    }

    /// Opens a kept value. Anything that does not open under this key and
    /// client, was issued 10 minutes or more before `now`, or was issued
    /// more than 60 seconds after `now`, is [`SignInRefusal::UnknownState`].
    pub(crate) fn open(&self, kept: &KeptState, now: SystemTime) -> Result<Pending, SignInRefusal> {
        let unknown = SignInRefusal::UnknownState;
        let mut bytes = URL_SAFE_NO_PAD.decode(kept.expose()).map_err(|_| unknown)?;
        let opened = self.open_bytes(&mut bytes, now);
        bytes.zeroize();
        opened.ok_or(unknown)
    }

    fn open_bytes(&self, bytes: &mut [u8], now: SystemTime) -> Option<Pending> {
        let (nonce, sealed) = bytes.split_at_mut_checked(NONCE_LEN)?;
        let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
        let aad = Aad::from(self.aad.as_slice());
        let plain = self.key.open_in_place(nonce, aad, sealed).ok()?;
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
        Some(Pending {
            issued,
            state: SecretString::from(state),
            nonce: SecretString::from(nonce),
            verifier: SecretString::from(verifier),
            return_to: ReturnTo::try_from(return_to).ok()?,
        })
    }
}

impl fmt::Debug for Sealer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Sealer")
            .field("sealed", &self.counter.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

/// Makes a pending sign-in, seals it, and builds the URL that starts it at
/// the provider.
pub(crate) fn begin(
    authorization: &Url,
    config: &OidcConfig,
    sealer: &Sealer,
    return_to: ReturnTo,
    now: SystemTime,
) -> Result<SignInStart, SignInRefusal> {
    let pending = Pending::new(return_to, now)?;
    let mut redirect = authorization.clone();
    {
        let mut query = redirect.query_pairs_mut();
        query
            .append_pair("response_type", "code")
            .append_pair("client_id", config.client_id.as_str())
            .append_pair("redirect_uri", config.redirect_uri.as_str())
            .append_pair("scope", "openid email profile")
            .append_pair("state", pending.state.expose_secret())
            .append_pair("nonce", pending.nonce.expose_secret())
            .append_pair(
                "code_challenge",
                &challenge(pending.verifier.expose_secret()),
            )
            .append_pair("code_challenge_method", "S256");
        // A hint only: the signed `hd` claim decides who is admitted.
        if let Some(domain) = config.admit.hosted_domain_hint() {
            query.append_pair("hd", domain);
        }
    }
    let kept = sealer.seal(&pending)?;
    Ok(SignInStart { redirect, kept })
}

/// What the provider's callback carries besides the state.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// An authorization code.
    Code(SecretString),
    /// An `error` parameter.
    Denied,
}

/// A parsed callback query.
#[derive(Debug)]
pub(crate) struct Callback {
    pub(crate) state: String,
    pub(crate) outcome: Outcome,
}

impl Callback {
    /// Parses the callback's raw query: at most 4,096 bytes, one non-empty
    /// `state`, and exactly one of `code` and `error`. Other parameters,
    /// such as `scope`, are ignored.
    pub(crate) fn parse(query: &str) -> Result<Self, SignInRefusal> {
        if query.len() > MAX_QUERY {
            return Err(SignInRefusal::Malformed);
        }
        let mut state = None;
        let mut code = None;
        let mut error = None;
        for (name, value) in url::form_urlencoded::parse(query.as_bytes()) {
            let slot = match name.as_ref() {
                "state" => &mut state,
                "code" => &mut code,
                "error" => &mut error,
                _ => continue,
            };
            if slot.replace(value.into_owned()).is_some() {
                return Err(SignInRefusal::Malformed);
            }
        }
        let state = state
            .filter(|state| !state.is_empty())
            .ok_or(SignInRefusal::Malformed)?;
        let outcome = match (code, error) {
            (Some(code), None) if !code.is_empty() => Outcome::Code(SecretString::from(code)),
            (None, Some(_)) => Outcome::Denied,
            _ => return Err(SignInRefusal::Malformed),
        };
        Ok(Self { state, outcome })
    }
}

#[cfg(test)]
mod tests;

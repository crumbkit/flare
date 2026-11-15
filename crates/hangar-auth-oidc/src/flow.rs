//! Pending sign-ins: the values that `start` makes, the table that keeps
//! them until the callback, and the callback query parser.

use crate::clock::has_passed;
use crate::config::{ConfigError, OidcConfig};
use crate::refusal::SignInRefusal;
use aws_lc_rs::{constant_time, digest, rand};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use secrecy::{ExposeSecret as _, SecretString};
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, SystemTime};
use url::Url;

/// How many sign-ins may be pending at once. A `start` at the limit removes
/// the oldest.
const MAX_PENDING: usize = 256;
/// How long a pending sign-in stays usable.
const PENDING_LIFETIME: Duration = Duration::from_secs(10 * 60);
/// The longest callback query that `complete` reads.
const MAX_QUERY: usize = 4096;
/// The longest return-to value.
const MAX_RETURN_TO: usize = 2048;

/// 32 random bytes in base64url, as a secret.
pub(crate) fn random_secret() -> Result<SecretString, SignInRefusal> {
    let mut bytes = [0_u8; 32];
    rand::fill(&mut bytes).map_err(|_| SignInRefusal::NoRandomness)?;
    Ok(SecretString::from(URL_SAFE_NO_PAD.encode(bytes)))
}

/// The SHA-256 of `text`, which the tables use as a key so they never hold
/// the state or session values themselves.
pub(crate) fn hash(text: &str) -> [u8; 32] {
    let mut key = [0_u8; 32];
    key.copy_from_slice(digest::digest(&digest::SHA256, text.as_bytes()).as_ref());
    key
}

/// The S256 PKCE challenge of `verifier` (RFC 7636 section 4.2).
pub(crate) fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, verifier.as_bytes()))
}

/// Where the server sends the person after the sign-in. The crate stores
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

/// The state value that the server keeps in a cookie between `start` and
/// `complete`. Its `Debug` output hides the value.
#[derive(Debug, Clone)]
pub struct KeptState(SecretString);

impl KeptState {
    /// The state as the browser sent it back in the cookie.
    #[must_use]
    pub fn from_cookie(value: &str) -> Self {
        Self(SecretString::from(value))
    }

    /// The value to put in the cookie.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.expose_secret()
    }

    /// Whether `state` equals the kept value, compared in constant time.
    pub(crate) fn matches(&self, state: &str) -> bool {
        constant_time::verify_slices_are_equal(self.0.expose_secret().as_bytes(), state.as_bytes())
            .is_ok()
    }
}

/// What `start` returns: the provider URL to redirect the browser to, and
/// the state for the server to keep in a cookie.
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

    /// The state to keep in a cookie until the callback.
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
    pub(crate) nonce: SecretString,
    pub(crate) verifier: SecretString,
    pub(crate) return_to: ReturnTo,
    created: SystemTime,
    order: u64,
}

/// Makes a pending sign-in and the URL that starts it at the provider.
/// Returns the start, the table key and the pending sign-in.
pub(crate) fn begin(
    authorization: &Url,
    config: &OidcConfig,
    return_to: ReturnTo,
    now: SystemTime,
) -> Result<(SignInStart, [u8; 32], Pending), SignInRefusal> {
    let state = random_secret()?;
    let nonce = random_secret()?;
    let verifier = random_secret()?;
    let mut redirect = authorization.clone();
    redirect
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", config.client_id.as_str())
        .append_pair("redirect_uri", config.redirect_uri.as_str())
        .append_pair("scope", "openid email profile")
        .append_pair("state", state.expose_secret())
        .append_pair("nonce", nonce.expose_secret())
        .append_pair("code_challenge", &challenge(verifier.expose_secret()))
        .append_pair("code_challenge_method", "S256");
    let key = hash(state.expose_secret());
    let pending = Pending {
        nonce,
        verifier,
        return_to,
        created: now,
        order: 0,
    };
    let start = SignInStart {
        redirect,
        kept: KeptState(state),
    };
    Ok((start, key, pending))
}

/// The pending sign-ins, keyed by the hash of their state.
#[derive(Debug, Default)]
pub(crate) struct PendingSignIns {
    entries: HashMap<[u8; 32], Pending>,
    next_order: u64,
}

impl PendingSignIns {
    /// Keeps `pending`. It first removes expired sign-ins, then the oldest
    /// while 256 are pending.
    pub(crate) fn insert(&mut self, key: [u8; 32], mut pending: Pending, now: SystemTime) {
        self.entries
            .retain(|_, kept| !has_passed(kept.created, PENDING_LIFETIME, now));
        while self.entries.len() >= MAX_PENDING {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, kept)| kept.order)
                .map(|(key, _)| *key);
            match oldest {
                Some(oldest) => self.entries.remove(&oldest),
                None => break,
            };
        }
        pending.order = self.next_order;
        self.next_order += 1;
        self.entries.insert(key, pending);
    }

    /// Removes and returns the sign-in of `key`, or `None` when there is
    /// none or it has expired.
    pub(crate) fn take(&mut self, key: &[u8; 32], now: SystemTime) -> Option<Pending> {
        let pending = self.entries.remove(key)?;
        (!has_passed(pending.created, PENDING_LIFETIME, now)).then_some(pending)
    }
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

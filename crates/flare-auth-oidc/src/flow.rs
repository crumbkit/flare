//! Pending sign-ins: the values that `start` makes, the sealed kept state
//! that carries them to the callback, and the callback query parser.
//!
//! A pending sign-in lives only in the browser's kept-state cookie, sealed
//! with AES-256-GCM under a key from the sealing key store (`seal.rs`), so
//! no `start` stores anything server-side.

use crate::clock::unix_seconds;
use crate::config::{ConfigError, OidcConfig};
use crate::refusal::SignInRefusal;
use crate::seal::Sealer;
use aws_lc_rs::{constant_time, digest, rand};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use secrecy::{ExposeSecret as _, SecretString};
use std::fmt;
use std::time::SystemTime;
use url::Url;

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
    /// A value the sealer made.
    pub(crate) fn from_sealed(value: String) -> Self {
        Self(SecretString::from(value))
    }

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

    /// The issue time, in seconds from the Unix epoch.
    pub(crate) fn issued(&self) -> u64 {
        self.issued
    }

    /// A pending sign-in read back from a kept value.
    pub(crate) fn from_parts(
        issued: u64,
        state: SecretString,
        nonce: SecretString,
        verifier: SecretString,
        return_to: ReturnTo,
    ) -> Self {
        Self {
            issued,
            state,
            nonce,
            verifier,
            return_to,
        }
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

/// Makes a pending sign-in, seals it, and builds the URL that starts it at
/// the provider.
pub(crate) async fn begin(
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
    let kept = sealer.seal(&pending, now).await?;
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

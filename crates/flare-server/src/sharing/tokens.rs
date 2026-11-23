//! Minting and reading CLI tokens and login codes (BASE-24). This is the
//! only file of the share API that calls `expose_secret`: to digest a
//! secret, to split a presented token into its id and secret, to put a new
//! token in the exchange's answer, and to put a login code in the redirect
//! to the CLI's loopback listener.

use crate::secret;
use aws_lc_rs::rand;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use flare_core::{Credential, Digest, TokenId};
use secrecy::{ExposeSecret as _, SecretString};

/// The prefix of every token's text.
const PREFIX: &str = "flare_";

/// The length of a secret or a code: 32 bytes in base64url.
const SECRET_CHARS: usize = 43;

/// `N` random bytes, or `None` when the operating system gives none.
fn random<const N: usize>() -> Option<[u8; N]> {
    let mut bytes = [0_u8; N];
    rand::fill(&mut bytes).ok()?;
    Some(bytes)
}

/// 16 random bytes for an id.
pub(super) fn random_id() -> Option<[u8; 16]> {
    random::<16>()
}

/// A new secret or login code: 32 random bytes in base64url.
pub(super) fn new_secret() -> Option<SecretString> {
    random::<32>().map(|bytes| SecretString::from(URL_SAFE_NO_PAD.encode(bytes)))
}

/// The digest that a store keeps for a secret or a code.
pub(super) fn digest(secret: &SecretString) -> Digest {
    Digest::from_bytes(secret::digest_of(secret))
}

/// The digest of a login code as a request carries it, when it has the
/// shape of one.
pub(super) fn code_digest(code: &str) -> Option<Digest> {
    let shaped = code.len() == SECRET_CHARS
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    shaped.then(|| Digest::from_bytes(secret::digest_of_text(code)))
}

/// The text of a token with `id` and `secret`.
pub(super) fn token_text(id: &TokenId, secret: &SecretString) -> String {
    format!("{PREFIX}{}_{}", id.as_str(), secret.expose_secret())
}

/// A presented token split into its id and the digest of its secret, or
/// `None` when the text is not a flare token.
pub(super) fn split(token: &SecretString) -> Option<(TokenId, Digest)> {
    let rest = token.expose_secret().strip_prefix(PREFIX)?;
    let (id, secret) = rest.split_once('_')?;
    let id = TokenId::try_from(id).ok()?;
    let digest = code_digest(secret)?;
    Some((id, digest))
}

/// Whether two digests are equal, compared in constant time.
pub(super) fn same(one: &Digest, other: &Digest) -> bool {
    secret::same(one.as_bytes(), other.as_bytes())
}

/// The bearer token a credential carries.
pub(crate) fn bearer_of(credential: &Credential) -> Option<&SecretString> {
    match credential {
        Credential::Bearer(secret) => Some(secret),
        _ => None,
    }
}

/// The redirect from `/cli/login` to the CLI's loopback listener, which
/// carries the login code and the CLI's state.
#[derive(Debug)]
pub(crate) struct LoginRedirect {
    port: u16,
    state: String,
    code: SecretString,
}

impl LoginRedirect {
    /// The redirect to the CLI on `port` with `state` and `code`.
    pub(crate) fn new(port: u16, state: String, code: SecretString) -> Self {
        Self { port, state, code }
    }

    /// The URL, `http://127.0.0.1:<port>/callback?code=<code>&state=<state>`.
    /// The state's characters need no escaping, because the handler admits
    /// only letters, digits, `-` and `_`, and the code is base64url.
    pub(crate) fn location(&self) -> String {
        format!(
            "http://127.0.0.1:{}/callback?code={}&state={}",
            self.port,
            self.code.expose_secret(),
            self.state
        )
    }
}

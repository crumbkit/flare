//! The server's secrets: form tokens, hand-over codes, the session cookie
//! of an OpenID Connect sign-in and the kept-state cookie (BASE-24). This
//! file is the only one in the crate's source, apart from unit tests, that
//! calls `expose_secret`. It composes every `Set-Cookie` that carries a
//! secret, and on the hosted site every `__Host-` cookie, set or cleared.
//!
//! The tables keep only SHA-256 digests. A secret leaves the process only
//! in a `Set-Cookie` header, a form's hidden input and the hand-over
//! redirect, each composed here.

use aws_lc_rs::{digest, rand};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hangar_auth_oidc::KeptState;
use hangar_core::Credential;
use hangar_render::FormToken;
use secrecy::{ExposeSecret as _, SecretString};
use std::time::Duration;

/// A SHA-256 digest, the key of every table.
pub(crate) type Digest = [u8; 32];

/// 32 random bytes from the operating system in base64url, or `None` when
/// it gives none.
pub(crate) fn random() -> Option<SecretString> {
    let mut bytes = [0_u8; 32];
    rand::fill(&mut bytes).ok()?;
    Some(SecretString::from(URL_SAFE_NO_PAD.encode(bytes)))
}

/// The SHA-256 of `text`.
pub(crate) fn sha256(text: &[u8]) -> Digest {
    let mut out = [0_u8; 32];
    out.copy_from_slice(digest::digest(&digest::SHA256, text).as_ref());
    out
}

/// The digest of a secret.
pub(crate) fn digest_of(secret: &SecretString) -> Digest {
    sha256(secret.expose_secret().as_bytes())
}

/// The digest of a form token as a post carries it.
pub(crate) fn digest_of_token(token: &FormToken) -> Digest {
    sha256(token.expose_secret().as_bytes())
}

/// The digest of a session credential's secret; `None` for a bearer token.
pub(crate) fn session_digest(credential: &Credential) -> Option<Digest> {
    match credential {
        Credential::Session(secret) => Some(digest_of(secret)),
        _ => None,
    }
}

/// A form token that carries `secret`.
pub(crate) fn form_token(secret: &SecretString) -> FormToken {
    FormToken::new(secret.expose_secret())
}

/// The `Set-Cookie` value that carries an OpenID Connect session on the
/// launch host: host-only, `HttpOnly`, `SameSite=Strict`, for as long as
/// the session lives.
pub(crate) fn session_cookie(credential: &Credential, max_age: Duration) -> Option<String> {
    match credential {
        Credential::Session(secret) => Some(format!(
            "hangar_session={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
            secret.expose_secret(),
            max_age.as_secs()
        )),
        _ => None,
    }
}

/// The `Set-Cookie` value of a loopback sign-in, which the loopback crate
/// composed.
pub(crate) fn loopback_cookie(set_cookie: &SecretString) -> String {
    set_cookie.expose_secret().to_owned()
}

/// The URL of the finish route that redeems `code` on `origin`.
pub(crate) fn finish_url(origin: &str, code: &SecretString) -> String {
    format!("{origin}/auth/oidc/finish?code={}", code.expose_secret())
}

/// The digest of a code as a request carries it.
pub(crate) fn digest_of_text(text: &str) -> Digest {
    sha256(text.as_bytes())
}

/// Whether two digests are equal, compared in constant time.
pub(crate) fn same(one: &Digest, other: &Digest) -> bool {
    aws_lc_rs::constant_time::verify_slices_are_equal(one, other).is_ok()
}

/// The `Set-Cookie` value that keeps an OpenID Connect sign-in's state on
/// `localhost` until the callback: `HttpOnly`, `SameSite=Lax` so the
/// provider's redirect back carries it, and only under `/auth/oidc`.
pub(crate) fn kept_cookie(kept: &KeptState) -> String {
    format!(
        "{KEPT_COOKIE}={}; Path=/auth/oidc; HttpOnly; SameSite=Lax; Max-Age=600",
        kept.expose()
    )
}

/// The name of the kept-state cookie.
pub(crate) const KEPT_COOKIE: &str = "hangar_oidc_state";

/// The name of the hosted site's session cookie. The `__Host-` prefix makes
/// a browser accept it only with `Secure`, `Path=/` and no `Domain`, so no
/// other host, a subdomain included, can set it.
pub(crate) const HOSTED_SESSION_COOKIE: &str = "__Host-hangar_session";

/// The name of the hosted site's kept-state cookie.
pub(crate) const HOSTED_KEPT_COOKIE: &str = "__Host-hangar_oidc_state";

/// The `Set-Cookie` value that carries a session on the hosted site:
/// `Secure`, `HttpOnly`, `SameSite=Lax` so links opened from other sites
/// carry it, for as long as the session lives.
pub(crate) fn hosted_session_cookie(secret: &SecretString, max_age: Duration) -> String {
    format!(
        "{HOSTED_SESSION_COOKIE}={}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age={}",
        secret.expose_secret(),
        max_age.as_secs()
    )
}

/// The `Set-Cookie` value that keeps a sealed sign-in on the hosted site
/// until the callback, for 600 seconds.
pub(crate) fn hosted_kept_cookie(kept: &KeptState) -> String {
    format!(
        "{HOSTED_KEPT_COOKIE}={}; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=600",
        kept.expose()
    )
}

/// The `Set-Cookie` value that clears the hosted kept-state cookie, with
/// the attributes that set it.
pub(crate) const HOSTED_CLEAR_KEPT: &str =
    "__Host-hangar_oidc_state=; Path=/; Secure; HttpOnly; SameSite=Lax; Max-Age=0";

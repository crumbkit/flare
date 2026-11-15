//! The code exchange and the ID token checks.

use crate::allow::Email;
use crate::clock::unix_seconds;
use crate::config::OidcConfig;
use crate::provider::{Provider, send};
use crate::refusal::{SignInRefusal, TokenFault};
use aws_lc_rs::constant_time;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use serde_json::Value;
use std::time::SystemTime;

/// How far past `exp` a token is still accepted, for clock skew.
const LEEWAY_SECONDS: u64 = 60;

/// The fields of the token reply that the crate reads. The access token is
/// never read, so it is dropped with the reply.
#[derive(Deserialize)]
struct TokenReply {
    id_token: Option<String>,
}

/// Exchanges the code for an ID token, sending the client secret and the
/// PKCE verifier in the body of the POST (`client_secret_post`).
pub(crate) async fn redeem(
    provider: &Provider,
    config: &OidcConfig,
    secret: &SecretString,
    code: &SecretString,
    verifier: &SecretString,
) -> Result<SecretString, SignInRefusal> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code.expose_secret()),
        ("redirect_uri", config.redirect_uri.as_str()),
        ("client_id", config.client_id.as_str()),
        ("client_secret", secret.expose_secret()),
        ("code_verifier", verifier.expose_secret()),
    ];
    let request = provider
        .http()
        .post(provider.endpoints.token.clone())
        .form(&form);
    let reply = send(request)
        .await
        .map_err(|_| SignInRefusal::ProviderUnavailable)?;
    match reply.status {
        200..=299 => {
            let parsed: TokenReply = serde_json::from_slice(&reply.body)
                .map_err(|_| SignInRefusal::ProviderMisbehaved)?;
            parsed
                .id_token
                .map(SecretString::from)
                .ok_or(SignInRefusal::ProviderMisbehaved)
        }
        // 408 and 429 say the provider is busy or slow, so the same
        // sign-in may succeed later.
        408 | 429 => Err(SignInRefusal::ProviderUnavailable),
        400..=499 => Err(SignInRefusal::CodeRejected),
        _ => Err(SignInRefusal::ProviderUnavailable),
    }
}

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: Option<String>,
}

/// Reads the token's header and returns its key id. The header's `alg`
/// must be RS256, and that is checked before any key is looked up.
pub(crate) fn key_id(token: &SecretString) -> Result<String, TokenFault> {
    let mut parts = token.expose_secret().split('.');
    let (Some(header), Some(_), Some(_), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(TokenFault::Malformed);
    };
    let header = URL_SAFE_NO_PAD
        .decode(header)
        .map_err(|_| TokenFault::Malformed)?;
    let header: Header = serde_json::from_slice(&header).map_err(|_| TokenFault::Malformed)?;
    if header.alg != "RS256" {
        return Err(TokenFault::Algorithm);
    }
    header.kid.ok_or(TokenFault::UnknownKey)
}

/// The claims the crate reads. Each is optional here so that a missing one
/// gives its own refusal.
#[derive(Deserialize)]
struct Claims {
    iss: Option<String>,
    aud: Option<Value>,
    azp: Option<String>,
    exp: Option<Value>,
    nonce: Option<String>,
    email: Option<String>,
    email_verified: Option<Value>,
}

/// What a token must say to be accepted.
pub(crate) struct Expected<'a> {
    pub(crate) config: &'a OidcConfig,
    pub(crate) nonce: &'a SecretString,
    pub(crate) now: SystemTime,
}

/// Google's issuer, whose ID tokens may name it without the scheme.
const GOOGLE_ISSUER: &str = "https://accounts.google.com";

/// Whether a token's `iss` names the configured issuer. It must equal it,
/// except that Google documents `accounts.google.com` as a second form of
/// `https://accounts.google.com`.
fn issuer_matches(configured: &str, iss: &str) -> bool {
    iss == configured || (configured == GOOGLE_ISSUER && iss == "accounts.google.com")
}

fn audience_is(aud: Option<&Value>, client_id: &str) -> bool {
    match aud {
        Some(Value::String(aud)) => aud == client_id,
        Some(Value::Array(auds)) => {
            matches!(auds.as_slice(), [Value::String(aud)] if aud == client_id)
        }
        _ => false,
    }
}

fn verify_signature(token: &SecretString, key: &DecodingKey) -> Result<Claims, TokenFault> {
    // jsonwebtoken checks only the signature and the algorithm here. The
    // claims are checked below, against the crate's clock.
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = false;
    validation.validate_aud = false;
    validation.required_spec_claims.clear();
    jsonwebtoken::decode::<Claims>(token.expose_secret(), key, &validation)
        .map(|data| data.claims)
        .map_err(|error| match error.kind() {
            ErrorKind::InvalidSignature => TokenFault::Signature,
            ErrorKind::InvalidAlgorithm => TokenFault::Algorithm,
            _ => TokenFault::Malformed,
        })
}

/// Verifies the token with `key`, checks its claims in order and returns
/// its verified email.
pub(crate) fn check(
    token: &SecretString,
    key: &DecodingKey,
    expected: &Expected<'_>,
) -> Result<Email, SignInRefusal> {
    let reject = SignInRefusal::TokenRejected;
    let claims = verify_signature(token, key).map_err(reject)?;
    let config = expected.config;
    let iss = claims.iss.unwrap_or_default();
    if !issuer_matches(config.issuer.as_str(), &iss) {
        return Err(reject(TokenFault::Issuer));
    }
    let client_id = config.client_id.as_str();
    let azp_differs = claims.azp.as_deref().is_some_and(|azp| azp != client_id);
    if !audience_is(claims.aud.as_ref(), client_id) || azp_differs {
        return Err(reject(TokenFault::Audience));
    }
    let exp = claims
        .exp
        .as_ref()
        .and_then(Value::as_u64)
        .ok_or(reject(TokenFault::Malformed))?;
    if unix_seconds(expected.now) > exp.saturating_add(LEEWAY_SECONDS) {
        return Err(reject(TokenFault::Expired));
    }
    let nonce = claims.nonce.unwrap_or_default();
    let wanted = expected.nonce.expose_secret();
    if constant_time::verify_slices_are_equal(nonce.as_bytes(), wanted.as_bytes()).is_err() {
        return Err(reject(TokenFault::Nonce));
    }
    let verified = claims.email_verified == Some(Value::Bool(true));
    let email = claims
        .email
        .filter(|_| verified)
        .ok_or(SignInRefusal::EmailUnverified)?;
    Email::try_from(email.as_str()).map_err(|_| SignInRefusal::NotAllowed)
}

#[cfg(test)]
mod tests;

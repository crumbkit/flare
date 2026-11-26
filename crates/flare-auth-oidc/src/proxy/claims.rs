//! The checks of a proxy's token: its size and header before any key is
//! looked up, then its signature and claims in the order the crate docs
//! give.

use super::config::ProxyConfig;
use crate::clock::unix_seconds;
use crate::identity::Identity;
use crate::refusal::ProxyRefusal;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use flare_core::{Email, Principal};
use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::time::SystemTime;

/// The longest token the crate decodes, in bytes.
const MAX_TOKEN: usize = 8 * 1024;
/// How far past `exp`, and how far ahead for `nbf` and `iat`, a token is
/// still accepted, for clock skew.
const LEEWAY_SECONDS: u64 = 60;

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: Option<String>,
}

/// Reads the token's header: its algorithm, which the config must list,
/// and its key id. Nothing is decoded from a token over 8 KiB.
pub(super) fn header(
    token: &str,
    config: &ProxyConfig,
) -> Result<(Algorithm, String), ProxyRefusal> {
    if token.len() > MAX_TOKEN {
        return Err(ProxyRefusal::TooLong);
    }
    let mut parts = token.split('.');
    let (Some(header), Some(_), Some(_), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(ProxyRefusal::Malformed);
    };
    let header = URL_SAFE_NO_PAD
        .decode(header)
        .map_err(|_| ProxyRefusal::Malformed)?;
    let header: Header = serde_json::from_slice(&header).map_err(|_| ProxyRefusal::Malformed)?;
    if !config.algorithms.lists(&header.alg) {
        return Err(ProxyRefusal::Algorithm);
    }
    let algorithm = match header.alg.as_str() {
        "ES256" => Algorithm::ES256,
        _ => Algorithm::RS256,
    };
    let kid = header.kid.ok_or(ProxyRefusal::UnknownKey)?;
    Ok((algorithm, kid))
}

/// Verifies the signature with `key` under `algorithm` alone. A key of
/// another family, such as an RSA key named by an ES256 header, is refused.
fn verify_signature(
    token: &str,
    key: &DecodingKey,
    algorithm: Algorithm,
) -> Result<Map<String, Value>, ProxyRefusal> {
    if key.family() != algorithm.family() {
        return Err(ProxyRefusal::Algorithm);
    }
    // jsonwebtoken checks only the signature and the algorithm here. The
    // claims are checked below, against the crate's clock.
    let mut validation = Validation::new(algorithm);
    validation.validate_exp = false;
    validation.validate_aud = false;
    validation.required_spec_claims.clear();
    jsonwebtoken::decode::<Map<String, Value>>(token, key, &validation)
        .map(|data| data.claims)
        .map_err(|error| match error.kind() {
            ErrorKind::InvalidSignature => ProxyRefusal::Signature,
            ErrorKind::InvalidAlgorithm => ProxyRefusal::Algorithm,
            _ => ProxyRefusal::Malformed,
        })
}

fn audience_contains(aud: Option<&Value>, audience: &str) -> bool {
    match aud {
        Some(Value::String(aud)) => aud == audience,
        Some(Value::Array(auds)) => auds
            .iter()
            .any(|aud| matches!(aud, Value::String(aud) if aud == audience)),
        _ => false,
    }
}

/// A time claim in whole seconds: `None` when absent, and malformed when
/// it is not a non-negative integer.
///
/// Shortcut: a fractional NumericDate, which RFC 7519 allows, is refused as
/// malformed. Proxies write whole seconds; a fractional value would need a
/// float comparison here.
fn seconds(claims: &Map<String, Value>, name: &str) -> Result<Option<u64>, ProxyRefusal> {
    match claims.get(name) {
        None => Ok(None),
        Some(value) => value.as_u64().map(Some).ok_or(ProxyRefusal::Malformed),
    }
}

/// Verifies the token with `key` and checks its claims in order, and
/// returns the principal the admit rule gives its address.
pub(super) fn verify(
    token: &str,
    key: &DecodingKey,
    algorithm: Algorithm,
    config: &ProxyConfig,
    now: SystemTime,
) -> Result<Principal, ProxyRefusal> {
    let claims = verify_signature(token, key, algorithm)?;
    if claims.get("iss").and_then(Value::as_str) != Some(config.issuer.as_str()) {
        return Err(ProxyRefusal::Issuer);
    }
    if !audience_contains(claims.get("aud"), config.audience.as_str()) {
        return Err(ProxyRefusal::Audience);
    }
    let now = unix_seconds(now);
    let exp = seconds(&claims, "exp")?.ok_or(ProxyRefusal::Malformed)?;
    if now > exp.saturating_add(LEEWAY_SECONDS) {
        return Err(ProxyRefusal::Expired);
    }
    for name in ["nbf", "iat"] {
        if seconds(&claims, name)?.is_some_and(|at| at > now.saturating_add(LEEWAY_SECONDS)) {
            return Err(ProxyRefusal::NotYetValid);
        }
    }
    if let Identity::Username(_) = config.identity {
        return admit_username(&claims, config);
    }
    let email = claims
        .get(config.identity_claim.as_str())
        .and_then(Value::as_str)
        .and_then(|text| Email::try_from(text).ok())
        .ok_or(ProxyRefusal::NoIdentity)?;
    if claims
        .get("email_verified")
        .is_some_and(|verified| *verified != Value::Bool(true))
    {
        return Err(ProxyRefusal::EmailUnverified);
    }
    config
        .admit
        .principal_of_address(email)
        .ok_or(ProxyRefusal::NotAdmitted)
}

/// The principal of a username token: the username of the configured
/// claim, with the token's `email` kept only when `email_verified` is true.
fn admit_username(
    claims: &Map<String, Value>,
    config: &ProxyConfig,
) -> Result<Principal, ProxyRefusal> {
    let username = config
        .identity
        .username(claims)
        .flatten()
        .ok_or(ProxyRefusal::NoIdentity)?;
    let email = claims
        .get("email")
        .and_then(Value::as_str)
        .filter(|_| claims.get("email_verified") == Some(&Value::Bool(true)))
        .and_then(|text| Email::try_from(text).ok());
    config
        .admit
        .principal_of_username(username, email)
        .ok_or(ProxyRefusal::NotAdmitted)
}

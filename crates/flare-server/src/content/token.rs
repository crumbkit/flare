//! The view token: what the reading page puts in the frame's address and the
//! content host opens.
//!
//! The token is `<key id>.<base64url of a 12-byte random nonce and the
//! ciphertext>`, sealed with AES-256-GCM under a key that HKDF-SHA256 derives
//! from a sealing key, with the info `flare-view-v1` and the same text as
//! the associated data. The plain text is JSON that names the share, the
//! version, the viewer and the issue and expiry times.
//!
//! Shortcut: the token carries the viewer's principal id, person and
//! verified address, so the content host can read the share as the viewer
//! without a session. A token is a bearer value for 10 minutes and is not
//! single-use, so a reload of the frame works. A store of used tokens would
//! make it single-use and would cost a write for each page view.

use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use aws_lc_rs::hkdf::{HKDF_SHA256, Salt};
use aws_lc_rs::rand;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use flare_core::{Email, Person, Principal, PrincipalId, SealingKey};
use serde_json::{Value, json};

/// How long a token works, in seconds.
pub(crate) const LIFETIME: u64 = 10 * 60;

/// How far ahead of the clock a token's issue time may lie, in seconds.
const AHEAD: u64 = 60;

/// The info of the key derivation and the associated data of the seal.
const LABEL: &[u8] = b"flare-view-v1";

/// What a token says: a viewer may read one version of one share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Claims {
    pub(crate) share: String,
    pub(crate) version: u32,
    pub(crate) viewer: Principal,
}

/// Why a token does not open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refused {
    /// The token is malformed, names an unreadable key, does not open, or
    /// was issued in the future.
    Invalid,
    /// The token opened and its expiry has passed.
    Expired,
}

/// The key that seals view tokens, derived from `key`.
fn derived(key: &SealingKey) -> Option<LessSafeKey> {
    let mut bytes = [0_u8; 32];
    let prk = Salt::new(HKDF_SHA256, &[]).extract(key.bytes.expose());
    prk.expand(&[LABEL], HKDF_SHA256)
        .ok()?
        .fill(&mut bytes)
        .ok()?;
    UnboundKey::new(&AES_256_GCM, &bytes)
        .ok()
        .map(LessSafeKey::new)
}

/// The JSON that a token seals.
fn plain(claims: &Claims, issued: u64) -> Vec<u8> {
    let viewer = &claims.viewer;
    json!({
        "s": claims.share,
        "v": claims.version,
        "p": viewer.id().as_str(),
        "w": viewer.person().map(Person::as_str),
        "m": viewer.email().map(Email::as_str),
        "i": issued,
        "x": issued.saturating_add(LIFETIME),
    })
    .to_string()
    .into_bytes()
}

/// Seals `claims`, issued at `issued` seconds since 1970, under `key`.
pub(crate) fn seal(key: &SealingKey, claims: &Claims, issued: u64) -> Option<String> {
    let aead = derived(key)?;
    let mut nonce = [0_u8; NONCE_LEN];
    rand::fill(&mut nonce).ok()?;
    let mut sealed = plain(claims, issued);
    aead.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(LABEL),
        &mut sealed,
    )
    .ok()?;
    let mut value = nonce.to_vec();
    value.extend_from_slice(&sealed);
    Some(format!(
        "{}.{}",
        key.id.get(),
        URL_SAFE_NO_PAD.encode(value)
    ))
}

/// The key id that `token` names, and the text after it.
pub(crate) fn split(token: &str) -> Option<(u32, &str)> {
    let (id, body) = token.split_once('.')?;
    Some((id.parse().ok()?, body))
}

/// Opens the text after the key id under `key`, at `now` seconds since 1970.
pub(crate) fn open(key: &SealingKey, body: &str, now: u64) -> Result<Claims, Refused> {
    let mut bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| Refused::Invalid)?;
    let aead = derived(key).ok_or(Refused::Invalid)?;
    let (nonce, sealed) = bytes
        .split_at_mut_checked(NONCE_LEN)
        .ok_or(Refused::Invalid)?;
    let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| Refused::Invalid)?;
    let plain = aead
        .open_in_place(nonce, Aad::from(LABEL), sealed)
        .map_err(|_| Refused::Invalid)?;
    let value: Value = serde_json::from_slice(plain).map_err(|_| Refused::Invalid)?;
    let number = |name: &str| value.get(name).and_then(Value::as_u64);
    let text = |name: &str| value.get(name).and_then(Value::as_str);
    let (issued, expires) = number("i").zip(number("x")).ok_or(Refused::Invalid)?;
    if issued > now.saturating_add(AHEAD) {
        return Err(Refused::Invalid);
    }
    if now >= expires {
        return Err(Refused::Expired);
    }
    let id = text("p")
        .and_then(|id| PrincipalId::try_from(id).ok())
        .ok_or(Refused::Invalid)?;
    let email = text("m").and_then(|text| Email::try_from(text).ok());
    let viewer = match text("w").map(Person::try_from) {
        Some(Ok(person)) => Principal::with_person(id, person, email),
        Some(Err(_)) => return Err(Refused::Invalid),
        None => Principal::new(id),
    };
    Ok(Claims {
        share: text("s").ok_or(Refused::Invalid)?.to_owned(),
        version: number("v")
            .and_then(|number| u32::try_from(number).ok())
            .ok_or(Refused::Invalid)?,
        viewer,
    })
}

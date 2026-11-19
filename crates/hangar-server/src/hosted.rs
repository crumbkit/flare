//! The hosted site's request checks, which run before the route table, in
//! this order:
//!
//! 1. Exactly one Host header, equal to the public origin's authority, or
//!    403 `request::host`. A proxy in front passes Host through unchanged.
//! 2. For any method other than GET and HEAD, exactly one Origin header
//!    equal to the public origin and exactly one `Sec-Fetch-Site:
//!    same-origin`, or 403 `request::origin`.
//! 3. No Authorization header, and at most one cookie pair named exactly
//!    `__Host-hangar_session` whose value is a token, or 401
//!    `auth::bad_credential`. Every other cookie pair is skipped, even one
//!    that is not ASCII, so a cookie set by a sibling domain cannot lock a
//!    browser out.
//!
//! The checks read no `X-Forwarded-*` or `Forwarded` header and take no
//! peer address: behind a proxy every peer is the proxy.

use crate::problem::Code;
use crate::secret::{HOSTED_KEPT_COOKIE, HOSTED_SESSION_COOKIE};
use crate::site::PublicOrigin;
use hangar_auth_oidc::KeptState;
use hangar_core::Credential;
use http::header::{AUTHORIZATION, COOKIE, HOST, HeaderName, ORIGIN};
use http::{HeaderMap, Method};

const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");

/// The one value of `name`, when the request has exactly one.
fn one<'h>(headers: &'h HeaderMap, name: &HeaderName) -> Option<&'h [u8]> {
    let mut values = headers.get_all(name).iter();
    match (values.next(), values.next()) {
        (Some(value), None) => Some(value.as_bytes()),
        _ => None,
    }
}

/// Runs the three checks and returns the session credential, if the
/// request carries one.
pub(crate) fn check(
    origin: &PublicOrigin,
    method: &Method,
    headers: &HeaderMap,
) -> Result<Option<Credential>, Code> {
    if one(headers, &HOST) != Some(origin.authority().as_bytes()) {
        return Err(Code::Host);
    }
    if method != Method::GET && method != Method::HEAD {
        let same_origin = one(headers, &ORIGIN) == Some(origin.as_str().as_bytes());
        let same_site = one(headers, &SEC_FETCH_SITE) == Some(b"same-origin".as_slice());
        if !same_origin || !same_site {
            return Err(Code::Origin);
        }
    }
    if headers.contains_key(AUTHORIZATION) {
        return Err(Code::BadCredential);
    }
    match cookie(headers, HOSTED_SESSION_COOKIE) {
        Cookie::Absent => Ok(None),
        Cookie::One(value) => Ok(Some(Credential::session(value))),
        Cookie::Bad => Err(Code::BadCredential),
    }
}

/// The hosted kept-state cookie, when the request carries exactly one with
/// a token value.
pub(crate) fn kept_state(headers: &HeaderMap) -> Option<KeptState> {
    match cookie(headers, HOSTED_KEPT_COOKIE) {
        Cookie::One(value) => Some(KeptState::from_cookie(value)),
        Cookie::Absent | Cookie::Bad => None,
    }
}

/// What the request's cookies hold under one name.
enum Cookie<'h> {
    /// No pair has the name.
    Absent,
    /// One pair has it, and its value is a token.
    One(&'h str),
    /// Two pairs have it, or its value is not a token.
    Bad,
}

/// Whether `value` is a cookie token: one or more visible ASCII bytes with
/// no comma, semicolon, double quote or backslash.
fn is_token(value: &[u8]) -> bool {
    !value.is_empty()
        && value
            .iter()
            .all(|byte| byte.is_ascii_graphic() && !b",;\"\\".contains(byte))
}

/// Reads the pairs of every Cookie header byte by byte. A pair is split at
/// its first `=` and its name compared exactly; a pair without `=` and a
/// pair with another name are skipped, whatever bytes they hold.
fn cookie<'h>(headers: &'h HeaderMap, name: &str) -> Cookie<'h> {
    let mut found = None;
    for header in headers.get_all(COOKIE) {
        for pair in header.as_bytes().split(|byte| *byte == b';') {
            let pair = pair.trim_ascii();
            let Some(at) = pair.iter().position(|byte| *byte == b'=') else {
                continue;
            };
            let (key, value) = pair.split_at(at);
            if key != name.as_bytes() {
                continue;
            }
            if found.is_some() {
                return Cookie::Bad;
            }
            found = Some(value.get(1..).unwrap_or_default());
        }
    }
    match found {
        None => Cookie::Absent,
        Some(value) if is_token(value) => {
            std::str::from_utf8(value).map_or(Cookie::Bad, Cookie::One)
        }
        Some(_) => Cookie::Bad,
    }
}

#[cfg(test)]
mod tests;

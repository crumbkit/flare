//! The hosted site's request checks, which run before the route table, in
//! this order:
//!
//! 1. Exactly one Host header, equal to the public origin's authority, or
//!    403 `request::host`. A proxy in front passes Host through unchanged.
//! 2. For any method other than GET and HEAD, exactly one Origin header
//!    equal to the public origin and exactly one `Sec-Fetch-Site:
//!    same-origin`, or 403 `request::origin`.
//! 3. No Authorization header, and at most one cookie pair named exactly
//!    `__Host-flare_session` whose value is a token, or 401
//!    `auth::bad_credential`. Every other cookie pair is skipped, even one
//!    that is not ASCII, so a cookie set by a sibling domain cannot lock a
//!    browser out.
//!
//! For a path under `/api/`, checks 2 and 3 are replaced. A request with an
//! Authorization header, or for a route that takes no cookie, has at most
//! one `Authorization: Bearer <token>` header as its credential, or 401
//! `auth::bad_credential`, and no cookie is read. A page on another site
//! can make a browser send cookies, and never an Authorization header, so
//! those routes cannot be driven through a signed-in browser. The comment
//! routes and `/api/activity`, when the request has no Authorization
//! header, read the session cookie as `page_request` says.
//!
//! The checks read no `X-Forwarded-*` or `Forwarded` header and take no
//! peer address: behind a proxy every peer is the proxy.

use crate::cookies::{Cookie, cookie};
use crate::problem::Code;
use crate::routes::{self, Found, Need};
use crate::secret::HOSTED_SESSION_COOKIE;
use crate::site::{PublicOrigin, SiteKind};
use flare_core::Credential;
use http::header::{AUTHORIZATION, CONTENT_TYPE, HOST, HeaderName, ORIGIN};
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
    path: &str,
    headers: &HeaderMap,
) -> Result<Option<Credential>, Code> {
    if one(headers, &HOST) != Some(origin.authority().as_bytes()) {
        return Err(Code::Host);
    }
    if path.starts_with("/api/") {
        if headers.contains_key(AUTHORIZATION) || !takes_cookie(method, path) {
            return bearer(headers);
        }
        return page_request(origin, method, headers);
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

/// Whether the route table lets this API request carry the session cookie.
fn takes_cookie(method: &Method, path: &str) -> bool {
    matches!(
        routes::find(method, path, SiteKind::Hosted),
        Found::Route(route, _) if route.credential == Need::TokenOrCookie
    )
}

/// The session cookie of a script on one of this site's pages, which is
/// the credential of an API request with no Authorization header. A
/// request that carries the cookie must say it comes from this site with
/// `Sec-Fetch-Site: same-origin`, and a write must also carry the exact
/// Origin and `Content-Type: application/json`, which a cross-site form
/// cannot send; otherwise it is refused as `request::origin`.
fn page_request(
    origin: &PublicOrigin,
    method: &Method,
    headers: &HeaderMap,
) -> Result<Option<Credential>, Code> {
    let value = match cookie(headers, HOSTED_SESSION_COOKIE) {
        Cookie::Absent => return Ok(None),
        Cookie::One(value) => value,
        Cookie::Bad => return Err(Code::BadCredential),
    };
    if one(headers, &SEC_FETCH_SITE) != Some(b"same-origin".as_slice()) {
        return Err(Code::Origin);
    }
    if method != Method::GET && method != Method::HEAD {
        let same_origin = one(headers, &ORIGIN) == Some(origin.as_str().as_bytes());
        let json = one(headers, &CONTENT_TYPE).is_some_and(is_json);
        if !same_origin || !json {
            return Err(Code::Origin);
        }
    }
    Ok(Some(Credential::session(value)))
}

/// Whether a Content-Type is `application/json`, with or without
/// parameters.
fn is_json(value: &[u8]) -> bool {
    let media = value.split(|byte| *byte == b';').next().unwrap_or_default();
    media.trim_ascii().eq_ignore_ascii_case(b"application/json")
}

/// The one bearer token of an API request, if it carries one.
fn bearer(headers: &HeaderMap) -> Result<Option<Credential>, Code> {
    if !headers.contains_key(AUTHORIZATION) {
        return Ok(None);
    }
    let value = one(headers, &AUTHORIZATION).ok_or(Code::BadCredential)?;
    let token = std::str::from_utf8(value)
        .ok()
        .and_then(|text| text.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty() && token.bytes().all(|byte| byte.is_ascii_graphic()))
        .ok_or(Code::BadCredential)?;
    Ok(Some(Credential::bearer(token)))
}

#[cfg(test)]
mod tests;

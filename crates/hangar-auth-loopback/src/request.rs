//! The checks that every request passes before a route runs.

use crate::host::LaunchHost;
use hangar_core::Credential;
use http::header::{AUTHORIZATION, COOKIE, HOST, ORIGIN};
use http::{HeaderMap, HeaderName, Method, StatusCode};
use std::net::IpAddr;

/// The cookie that holds a browser's session secret.
pub(crate) const SESSION_COOKIE: &str = "hangar_session";

const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");

/// A request that passed [`Loopback::check`](crate::Loopback::check), and
/// the credential it presents, if any.
#[derive(Debug)]
#[non_exhaustive]
pub struct Checked {
    credential: Option<Credential>,
}

impl Checked {
    /// The credential, for [`Authenticator::authenticate`](hangar_core::Authenticator::authenticate).
    /// `None` when the request presents none.
    #[must_use]
    pub fn credential(&self) -> Option<&Credential> {
        self.credential.as_ref()
    }

    /// The credential, by value.
    #[must_use]
    pub fn into_credential(self) -> Option<Credential> {
        self.credential
    }
}

/// Why a request was refused before any route ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Refusal {
    /// The peer is not a loopback address.
    #[error("the request did not come from this machine")]
    Peer,
    /// The request has no `Host` header, more than one, or one that is not
    /// the launch host and port. This refuses DNS rebinding.
    #[error("the request's Host header is not this server's launch host")]
    Host,
    /// A request that changes something came from another origin, or did
    /// not say where it came from.
    #[error("the request's Origin or Sec-Fetch-Site header is not this server's")]
    Origin,
    /// The `Authorization` header is malformed or repeated, or the request
    /// has two session cookies.
    #[error("the request's credential is malformed or repeated")]
    Credential,
}

impl Refusal {
    /// The status to answer with: 401 for [`Refusal::Credential`] and 403
    /// for the others.
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Peer | Self::Host | Self::Origin => StatusCode::FORBIDDEN,
            Self::Credential => StatusCode::UNAUTHORIZED,
        }
    }
}

/// Runs the four checks in order: peer, Host, Origin, credential.
pub(crate) fn check(
    host: &LaunchHost,
    peer: IpAddr,
    method: &Method,
    headers: &HeaderMap,
) -> Result<Checked, Refusal> {
    if !peer.to_canonical().is_loopback() {
        return Err(Refusal::Peer);
    }
    let authority = host.authority();
    if !only(headers, &HOST).is_some_and(|value| value == authority.as_bytes()) {
        return Err(Refusal::Host);
    }
    if !matches!(*method, Method::GET | Method::HEAD) {
        check_origin(host, headers)?;
    }
    let credential = credential(headers)?;
    Ok(Checked { credential })
}

/// The value of `name` when the request has exactly one such header.
fn only<'h>(headers: &'h HeaderMap, name: &HeaderName) -> Option<&'h [u8]> {
    let mut values = headers.get_all(name).iter();
    let first = values.next()?;
    values.next().is_none().then(|| first.as_bytes())
}

/// A request that changes something needs `Origin` equal to the launch
/// origin and `Sec-Fetch-Site: same-origin`. A request with an
/// `Authorization` header and neither of those headers passes, because a
/// browser never adds an `Authorization` header by itself.
fn check_origin(host: &LaunchHost, headers: &HeaderMap) -> Result<(), Refusal> {
    let has_origin = headers.contains_key(ORIGIN);
    let has_site = headers.contains_key(SEC_FETCH_SITE);
    if headers.contains_key(AUTHORIZATION) && !has_origin && !has_site {
        return Ok(());
    }
    let origin = host.origin();
    let origin_matches = only(headers, &ORIGIN).is_some_and(|value| value == origin.as_bytes());
    let site_matches = only(headers, &SEC_FETCH_SITE).is_some_and(|value| value == b"same-origin");
    if origin_matches && site_matches {
        Ok(())
    } else {
        Err(Refusal::Origin)
    }
}

/// `Authorization: Bearer <token>` gives a bearer credential. Without an
/// `Authorization` header, the `hangar_session` cookie gives a session
/// credential.
fn credential(headers: &HeaderMap) -> Result<Option<Credential>, Refusal> {
    let mut authorizations = headers.get_all(AUTHORIZATION).iter();
    if let Some(authorization) = authorizations.next() {
        if authorizations.next().is_some() {
            return Err(Refusal::Credential);
        }
        let text = authorization.to_str().map_err(|_| Refusal::Credential)?;
        let token = text
            .strip_prefix("Bearer ")
            .filter(|token| is_token(token))
            .ok_or(Refusal::Credential)?;
        return Ok(Some(Credential::bearer(token)));
    }
    let mut found = None;
    for header in &headers.get_all(COOKIE) {
        let text = header.to_str().map_err(|_| Refusal::Credential)?;
        for pair in text.split(';') {
            let Some((name, value)) = pair.trim().split_once('=') else {
                continue;
            };
            if name != SESSION_COOKIE {
                continue;
            }
            if found.is_some() || !is_token(value) {
                return Err(Refusal::Credential);
            }
            found = Some(Credential::session(value));
        }
    }
    Ok(found)
}

/// A token or cookie value: at least one visible ASCII character, with no
/// space, comma, semicolon or quote.
fn is_token(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b',' | b';' | b'"' | b'\\'))
}

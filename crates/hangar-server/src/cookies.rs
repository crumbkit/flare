//! Reads one cookie from a request's Cookie headers, byte by byte, on both
//! sites. A pair with another name is skipped whatever bytes it holds, so a
//! non-ASCII cookie set by another app or a sibling domain cannot hide
//! hangar's own cookie.

use hangar_auth_oidc::KeptState;
use http::HeaderMap;
use http::header::COOKIE;

/// What the request's cookies hold under one name.
pub(crate) enum Cookie<'h> {
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
pub(crate) fn cookie<'h>(headers: &'h HeaderMap, name: &str) -> Cookie<'h> {
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

/// The kept-state cookie named `name`, when the request carries exactly one
/// with a token value.
pub(crate) fn kept_state(headers: &HeaderMap, name: &str) -> Option<KeptState> {
    match cookie(headers, name) {
        Cookie::One(value) => Some(KeptState::from_cookie(value)),
        Cookie::Absent | Cookie::Bad => None,
    }
}

#[cfg(test)]
mod tests;

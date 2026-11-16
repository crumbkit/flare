//! The listener's checks that `Loopback::check` does not make: the size of
//! the header block, the sign-in host's Host header, and credentialed
//! cross-site requests.

use crate::problem::Code;
use hangar_auth_loopback::Refusal;
use http::header::{HOST, HeaderName};
use http::{HeaderMap, Uri};
use std::net::IpAddr;

const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
const SEC_FETCH_MODE: HeaderName = HeaderName::from_static("sec-fetch-mode");
const SEC_FETCH_DEST: HeaderName = HeaderName::from_static("sec-fetch-dest");

/// The bytes the header block takes: each name, value, colon, space and
/// line end.
pub(crate) fn header_bytes(headers: &HeaderMap) -> usize {
    headers
        .iter()
        .map(|(name, value)| name.as_str().len() + value.len() + 4)
        .sum()
}

/// The code for a refusal of `Loopback::check`.
pub(crate) fn refusal(refusal: Refusal) -> Code {
    match refusal {
        Refusal::Peer => Code::Peer,
        Refusal::Host => Code::Host,
        Refusal::Origin => Code::Origin,
        _ => Code::BadCredential,
    }
}

/// The sign-in host's checks: the peer is a loopback address and the one
/// Host header is `localhost:<port>`.
pub(crate) fn sign_in_host(peer: IpAddr, headers: &HeaderMap, port: u16) -> Result<(), Code> {
    if !peer.to_canonical().is_loopback() {
        return Err(Code::Peer);
    }
    let mut hosts = headers.get_all(HOST).iter();
    let expected = format!("localhost:{port}");
    match (hosts.next(), hosts.next()) {
        (Some(host), None) if host.as_bytes() == expected.as_bytes() => Ok(()),
        _ => Err(Code::Host),
    }
}

/// Refuses a request that carries a credential and that the browser marks
/// as cross-site, unless it is a navigation. A sandboxed frame's
/// subresource requests are cross-site, and Safari sends the session cookie
/// on them; the launch and finish routes are reached by cross-site
/// navigations.
///
/// It also refuses a credentialed request whose `Sec-Fetch-Dest` is
/// `iframe` or `frame`: a link in a page's content, clicked inside the
/// sandboxed frame, would load `/b/<name>` in the frame with the session.
/// No hangar document is ever framed, and the frame's `srcdoc` sends no
/// request, so nothing a reader does is refused by this.
pub(crate) fn cross_site(headers: &HeaderMap, has_credential: bool) -> Result<(), Code> {
    if !has_credential {
        return Ok(());
    }
    let is = |name: &HeaderName, value: &[u8]| {
        headers
            .get_all(name)
            .iter()
            .any(|header| header.as_bytes().eq_ignore_ascii_case(value))
    };
    if is(&SEC_FETCH_DEST, b"iframe") || is(&SEC_FETCH_DEST, b"frame") {
        return Err(Code::Framed);
    }
    if is(&SEC_FETCH_SITE, b"cross-site") && !is(&SEC_FETCH_MODE, b"navigate") {
        return Err(Code::CrossSite);
    }
    Ok(())
}

/// The one value of `name` in the URI's query, decoded; `None` when it is
/// missing or given twice.
pub(crate) fn query_value(uri: &Uri, name: &str) -> Option<String> {
    let query = uri.query()?;
    let mut values = form_urlencoded::parse(query.as_bytes())
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned());
    let first = values.next()?;
    values.next().is_none().then_some(first)
}

#[cfg(test)]
mod tests;

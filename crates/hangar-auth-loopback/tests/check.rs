//! `Loopback::check`: the peer, Host, Origin and credential checks that run
//! before any route.
#![cfg(test)]

#[path = "support/fixture.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the fixture is test support: it panics on a broken fixture, and each test file uses part of it"
)]
pub mod fixture;

use fixture::{PORT, loopback};
use hangar_auth_loopback::{Checked, Loopback, Refusal};
use hangar_core::Credential;
use http::header::{AUTHORIZATION, COOKIE, HOST, ORIGIN};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use secrecy::ExposeSecret as _;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const LOCAL: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
const SECRET: &str = "5e55105e55105e55105e55105e55105e55105e55105e55105e55105e55105e55";

fn authority(loopback: &Loopback) -> String {
    format!("{}:{PORT}", loopback.launch_host().name())
}

fn origin(loopback: &Loopback) -> String {
    format!("http://{}", authority(loopback))
}

/// Headers with the launch Host and the given extra headers.
fn headers(loopback: &Loopback, extra: &[(HeaderName, &str)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(HOST, HeaderValue::from_str(&authority(loopback)).unwrap());
    for (name, value) in extra {
        headers.append(name.clone(), HeaderValue::from_str(value).unwrap());
    }
    headers
}

fn cookie() -> (HeaderName, String) {
    (COOKIE, format!("flare_session={SECRET}"))
}

fn check(loopback: &Loopback, method: &Method, headers: &HeaderMap) -> Result<Checked, Refusal> {
    loopback.check(LOCAL, method, headers)
}

fn session_of(checked: &Checked) -> Option<String> {
    match checked.credential()? {
        Credential::Session(secret) => Some(secret.expose_secret().to_owned()),
        other => panic!("expected a session, got {other:?}"),
    }
}

#[test]
fn the_launch_host_is_h_and_32_hex_digits_under_localhost() {
    let name = loopback().launch_host().name().to_owned();
    let hex = name
        .strip_prefix("h-")
        .and_then(|rest| rest.strip_suffix(".localhost"))
        .unwrap();
    assert_eq!(hex.len(), 32);
    assert!(
        hex.bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    );
    assert_eq!(loopback().launch_host().port(), PORT);
}

#[test]
fn two_servers_get_two_launch_hosts() {
    assert_ne!(
        loopback().launch_host().name(),
        loopback().launch_host().name()
    );
}

#[test]
fn the_exact_launch_host_passes() {
    let loopback = loopback();
    let checked = check(&loopback, &Method::GET, &headers(&loopback, &[])).unwrap();
    assert!(checked.credential().is_none());
}

#[test]
fn a_host_other_than_the_launch_host_is_refused() {
    let loopback = loopback();
    let other_port = format!("{}:{}", loopback.launch_host().name(), PORT + 1);
    for host in [
        "evil.example",
        &format!("127.0.0.1:{PORT}"),
        &format!("localhost:{PORT}"),
        loopback.launch_host().name(),
        &other_port,
    ] {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_str(host).unwrap());
        assert_eq!(
            check(&loopback, &Method::GET, &headers).unwrap_err(),
            Refusal::Host,
            "{host}"
        );
    }
}

#[test]
fn a_missing_host_and_two_hosts_are_refused() {
    let loopback = loopback();
    assert_eq!(
        check(&loopback, &Method::GET, &HeaderMap::new()).unwrap_err(),
        Refusal::Host
    );
    let mut twice = headers(&loopback, &[]);
    twice.append(HOST, HeaderValue::from_str(&authority(&loopback)).unwrap());
    assert_eq!(
        check(&loopback, &Method::GET, &twice).unwrap_err(),
        Refusal::Host
    );
}

#[test]
fn a_peer_that_is_not_a_loopback_address_is_refused() {
    let loopback = loopback();
    let headers = headers(&loopback, &[]);
    let remote = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 7));
    assert_eq!(
        loopback.check(remote, &Method::GET, &headers).unwrap_err(),
        Refusal::Peer
    );
}

#[test]
fn the_ipv4_and_ipv6_loopback_addresses_pass() {
    let loopback = loopback();
    let headers = headers(&loopback, &[]);
    for peer in [LOCAL, IpAddr::V6(Ipv6Addr::LOCALHOST)] {
        assert!(
            loopback.check(peer, &Method::GET, &headers).is_ok(),
            "{peer}"
        );
    }
}

#[test]
fn a_post_with_a_cookie_passes_with_the_exact_origin_and_same_origin() {
    let loopback = loopback();
    let (name, value) = cookie();
    let origin = origin(&loopback);
    let headers = headers(
        &loopback,
        &[
            (name, &value),
            (ORIGIN, &origin),
            (SEC_FETCH_SITE, "same-origin"),
        ],
    );
    let checked = check(&loopback, &Method::POST, &headers).unwrap();
    assert_eq!(session_of(&checked).as_deref(), Some(SECRET));
}

#[test]
fn a_post_with_a_cookie_is_refused_unless_sec_fetch_site_is_same_origin() {
    let loopback = loopback();
    let (name, value) = cookie();
    let origin = origin(&loopback);
    for site in [Some("same-site"), Some("cross-site"), None] {
        let mut extra = vec![(name.clone(), value.as_str()), (ORIGIN, origin.as_str())];
        if let Some(site) = site {
            extra.push((SEC_FETCH_SITE, site));
        }
        assert_eq!(
            check(&loopback, &Method::POST, &headers(&loopback, &extra)).unwrap_err(),
            Refusal::Origin,
            "{site:?}"
        );
    }
}

#[test]
fn a_post_with_a_cookie_is_refused_without_the_exact_origin() {
    let loopback = loopback();
    let (name, value) = cookie();
    let other_port = format!("http://{}:{}", loopback.launch_host().name(), PORT + 1);
    let numeric = format!("http://127.0.0.1:{PORT}");
    for origin in [None, Some(other_port.as_str()), Some(numeric.as_str())] {
        let mut extra = vec![
            (name.clone(), value.as_str()),
            (SEC_FETCH_SITE, "same-origin"),
        ];
        if let Some(origin) = origin {
            extra.push((ORIGIN, origin));
        }
        assert_eq!(
            check(&loopback, &Method::POST, &headers(&loopback, &extra)).unwrap_err(),
            Refusal::Origin,
            "{origin:?}"
        );
    }
}

#[test]
fn a_post_with_a_bearer_token_and_neither_origin_header_passes() {
    let loopback = loopback();
    let authorization = format!("Bearer {SECRET}");
    let headers = headers(&loopback, &[(AUTHORIZATION, &authorization)]);
    let checked = check(&loopback, &Method::POST, &headers).unwrap();
    let Some(Credential::Bearer(token)) = checked.credential() else {
        panic!("expected a bearer credential");
    };
    assert_eq!(token.expose_secret(), SECRET);
}

#[test]
fn a_post_with_a_bearer_token_and_a_wrong_origin_is_refused() {
    let loopback = loopback();
    let authorization = format!("Bearer {SECRET}");
    let headers = headers(
        &loopback,
        &[
            (AUTHORIZATION, &authorization),
            (ORIGIN, "http://evil.example"),
        ],
    );
    assert_eq!(
        check(&loopback, &Method::POST, &headers).unwrap_err(),
        Refusal::Origin
    );
}

#[test]
fn a_post_with_a_bearer_token_and_both_right_origin_headers_passes() {
    let loopback = loopback();
    let authorization = format!("Bearer {SECRET}");
    let origin = origin(&loopback);
    let headers = headers(
        &loopback,
        &[
            (AUTHORIZATION, &authorization),
            (ORIGIN, &origin),
            (SEC_FETCH_SITE, "same-origin"),
        ],
    );
    assert!(check(&loopback, &Method::POST, &headers).is_ok());
}

#[test]
fn a_get_with_a_cookie_and_no_origin_passes() {
    let loopback = loopback();
    let (name, value) = cookie();
    for method in [Method::GET, Method::HEAD] {
        let checked = check(
            &loopback,
            &method,
            &headers(&loopback, &[(name.clone(), &value)]),
        )
        .unwrap();
        assert_eq!(session_of(&checked).as_deref(), Some(SECRET), "{method}");
    }
}

#[test]
fn the_session_cookie_is_found_among_other_cookies() {
    let loopback = loopback();
    let one_header = format!("theme=dark; flare_session={SECRET}; lang=en");
    let checked = check(
        &loopback,
        &Method::GET,
        &headers(&loopback, &[(COOKIE, &one_header)]),
    )
    .unwrap();
    assert_eq!(session_of(&checked).as_deref(), Some(SECRET));
    let session = format!("flare_session={SECRET}");
    let split = headers(&loopback, &[(COOKIE, "theme=dark"), (COOKIE, &session)]);
    let checked = check(&loopback, &Method::GET, &split).unwrap();
    assert_eq!(session_of(&checked).as_deref(), Some(SECRET));
}

#[test]
fn two_session_cookies_are_refused() {
    let loopback = loopback();
    let twice = format!("flare_session={SECRET}; flare_session={SECRET}");
    assert_eq!(
        check(
            &loopback,
            &Method::GET,
            &headers(&loopback, &[(COOKIE, &twice)])
        )
        .unwrap_err(),
        Refusal::Credential
    );
}

#[test]
fn a_malformed_authorization_header_is_refused() {
    let loopback = loopback();
    for value in ["Basic YWRhOnB3", "Bearer", "Bearer ", "Bearer a b", SECRET] {
        assert_eq!(
            check(
                &loopback,
                &Method::GET,
                &headers(&loopback, &[(AUTHORIZATION, value)])
            )
            .unwrap_err(),
            Refusal::Credential,
            "{value}"
        );
    }
}

#[test]
fn two_authorization_headers_are_refused() {
    let loopback = loopback();
    let authorization = format!("Bearer {SECRET}");
    let headers = headers(
        &loopback,
        &[
            (AUTHORIZATION, &authorization),
            (AUTHORIZATION, &authorization),
        ],
    );
    assert_eq!(
        check(&loopback, &Method::GET, &headers).unwrap_err(),
        Refusal::Credential
    );
}

#[test]
fn each_refusal_names_its_status() {
    assert_eq!(Refusal::Peer.status(), StatusCode::FORBIDDEN);
    assert_eq!(Refusal::Host.status(), StatusCode::FORBIDDEN);
    assert_eq!(Refusal::Origin.status(), StatusCode::FORBIDDEN);
    assert_eq!(Refusal::Credential.status(), StatusCode::UNAUTHORIZED);
}

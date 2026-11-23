use super::check;
use crate::problem::Code;
use crate::site::PublicOrigin;
use hangar_core::Credential;
use http::header::COOKIE;
use http::{HeaderMap, HeaderValue, Method};
use secrecy::ExposeSecret as _;

const AUTHORITY: &str = "flare.example.com";
const ORIGIN_TEXT: &str = "https://flare.example.com";

fn public() -> PublicOrigin {
    PublicOrigin::try_from(ORIGIN_TEXT).unwrap()
}

fn headers(pairs: &[(&str, &[u8])]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.append(
            http::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_bytes(value).unwrap(),
        );
    }
    map
}

fn secret_of(credential: Option<Credential>) -> Option<String> {
    match credential {
        Some(Credential::Session(secret)) => Some(secret.expose_secret().to_owned()),
        _ => None,
    }
}

#[test]
fn the_checks_read_no_peer_or_forwarded_header_so_a_request_from_any_address_passes() {
    let map = headers(&[
        ("host", AUTHORITY.as_bytes()),
        ("x-forwarded-for", b"192.0.2.1"),
        ("x-forwarded-host", b"evil.example"),
        ("forwarded", b"for=192.0.2.1;host=evil.example"),
    ]);
    assert_eq!(
        check(&public(), &Method::GET, &map).map(secret_of),
        Ok(None)
    );
}

#[test]
fn the_host_is_checked_before_the_origin_and_the_origin_before_the_credential() {
    let everything_wrong = headers(&[
        ("host", b"evil.example"),
        ("origin", b"https://evil.example"),
        ("authorization", b"Bearer x"),
    ]);
    assert_eq!(
        check(&public(), &Method::POST, &everything_wrong).map(secret_of),
        Err(Code::Host)
    );
    let origin_and_credential_wrong = headers(&[
        ("host", AUTHORITY.as_bytes()),
        ("origin", b"https://evil.example"),
        ("authorization", b"Bearer x"),
    ]);
    assert_eq!(
        check(&public(), &Method::POST, &origin_and_credential_wrong).map(secret_of),
        Err(Code::Origin)
    );
    let credential_wrong = headers(&[
        ("host", AUTHORITY.as_bytes()),
        ("origin", ORIGIN_TEXT.as_bytes()),
        ("sec-fetch-site", b"same-origin"),
        ("authorization", b"Bearer x"),
    ]);
    assert_eq!(
        check(&public(), &Method::POST, &credential_wrong).map(secret_of),
        Err(Code::BadCredential)
    );
    let right = headers(&[
        ("host", AUTHORITY.as_bytes()),
        ("origin", ORIGIN_TEXT.as_bytes()),
        ("sec-fetch-site", b"same-origin"),
        ("cookie", b"__Host-flare_session=abc"),
    ]);
    assert_eq!(
        check(&public(), &Method::POST, &right).map(secret_of),
        Ok(Some("abc".to_owned()))
    );
}

#[test]
fn a_get_or_head_needs_no_origin_and_any_other_method_does() {
    let bare = headers(&[("host", AUTHORITY.as_bytes())]);
    for method in [Method::GET, Method::HEAD] {
        assert_eq!(check(&public(), &method, &bare).map(secret_of), Ok(None));
    }
    for method in [Method::POST, Method::PUT, Method::DELETE, Method::PATCH] {
        assert_eq!(
            check(&public(), &method, &bare).map(secret_of),
            Err(Code::Origin),
            "{method}"
        );
    }
}

#[test]
fn the_session_cookie_is_read_from_any_cookie_header_beside_unparseable_pairs() {
    let map = headers(&[
        ("host", AUTHORITY.as_bytes()),
        (COOKIE.as_str(), b"theme=caf\xc3\xa9; bare; =x"),
        (
            COOKIE.as_str(),
            b"other=1;__Host-flare_session=tok-en_1  ; z=\xff",
        ),
    ]);
    assert_eq!(
        check(&public(), &Method::GET, &map).map(secret_of),
        Ok(Some("tok-en_1".to_owned()))
    );
}

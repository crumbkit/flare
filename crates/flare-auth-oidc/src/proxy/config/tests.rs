//! Unit tests of the proxy config's values: each refusal beside the value
//! it admits.

use super::*;

#[test]
fn an_https_key_set_url_is_admitted_and_an_http_one_refused() {
    assert!(KeySetUrl::try_from("https://sso.example.com/.well-known/jwks.json").is_ok());
    for text in [
        "http://sso.example.com/jwks",
        "https://user@sso.example.com/jwks",
        "https://sso.example.com/jwks#keys",
        "not a url",
    ] {
        assert_eq!(
            KeySetUrl::try_from(text),
            Err(ProxyConfigError::KeySetUrl),
            "{text}"
        );
    }
}

#[test]
fn only_a_loopback_http_key_set_url_is_admitted_for_tests() {
    assert!(KeySetUrl::loopback_http("http://127.0.0.1:9000/jwks").is_ok());
    assert_eq!(
        KeySetUrl::loopback_http("http://sso.example.com/jwks"),
        Err(ProxyConfigError::KeySetUrl)
    );
}

#[test]
fn an_issuer_and_an_audience_are_visible_ascii_of_at_most_512_bytes() {
    assert!(ProxyIssuer::try_from("https://sso.example.com").is_ok());
    assert!(Audience::try_from("flare").is_ok());
    for text in ["", "has space", &"a".repeat(513)] {
        assert_eq!(
            ProxyIssuer::try_from(text),
            Err(ProxyConfigError::Issuer),
            "{text}"
        );
        assert_eq!(
            Audience::try_from(text),
            Err(ProxyConfigError::Audience),
            "{text}"
        );
    }
}

#[test]
fn a_cookie_name_is_a_token_and_a_separator_is_refused() {
    assert!(CookieName::try_from("__Host-proxy_token").is_ok());
    for text in ["", "a=b", "a;b", "a b", "a,b", "\"a\""] {
        assert_eq!(
            CookieName::try_from(text),
            Err(ProxyConfigError::Cookie),
            "{text}"
        );
    }
}

#[test]
fn an_identity_claim_is_a_short_name() {
    assert_eq!(IdentityClaim::default().as_str(), "email");
    assert!(IdentityClaim::try_from("preferred_username").is_ok());
    for text in ["", "a b", &"a".repeat(65)] {
        assert_eq!(
            IdentityClaim::try_from(text),
            Err(ProxyConfigError::IdentityClaim),
            "{text}"
        );
    }
}

#[test]
fn a_link_must_be_https() {
    assert!(ProxyLink::try_from("https://sso.example.com/sign-in").is_ok());
    assert_eq!(
        ProxyLink::try_from("http://sso.example.com/sign-in"),
        Err(ProxyConfigError::Link)
    );
    assert_eq!(
        ProxyLink::try_from("javascript:alert(1)"),
        Err(ProxyConfigError::Link)
    );
}

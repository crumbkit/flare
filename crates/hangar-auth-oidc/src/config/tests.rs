//! Unit tests of the issuer, client ID and redirect URI parsers.

use super::*;

#[test]
fn an_https_issuer_is_admitted_and_kept_exactly_as_written() {
    let issuer = IssuerUrl::try_from("https://accounts.google.com").unwrap();
    assert_eq!(issuer.as_str(), "https://accounts.google.com");
    assert_eq!(
        issuer.discovery_url().unwrap().as_str(),
        "https://accounts.google.com/.well-known/openid-configuration"
    );
    let slashed = IssuerUrl::try_from("https://id.example.net/tenant/").unwrap();
    assert_eq!(
        slashed.discovery_url().unwrap().as_str(),
        "https://id.example.net/tenant/.well-known/openid-configuration"
    );
}

#[test]
fn an_issuer_with_http_a_query_a_fragment_or_a_user_is_refused() {
    for text in [
        "http://accounts.google.com",
        "https://accounts.google.com/?a=b",
        "https://accounts.google.com/#top",
        "https://user@accounts.google.com",
        "not a url",
        "",
    ] {
        assert_eq!(
            IssuerUrl::try_from(text),
            Err(ConfigError::Issuer),
            "{text}"
        );
    }
}

#[test]
fn a_client_id_of_visible_ascii_is_admitted_and_one_with_spaces_or_none_is_refused() {
    assert!(ClientId::try_from("123-abc.apps.googleusercontent.com").is_ok());
    for text in ["", "has space", "tab\t", &"x".repeat(257)] {
        assert_eq!(
            ClientId::try_from(text),
            Err(ConfigError::ClientId),
            "{text}"
        );
    }
}

#[test]
fn a_redirect_uri_over_https_or_loopback_http_is_admitted() {
    for text in [
        "https://hangar.example.net/auth/oidc/callback",
        "http://localhost:8740/auth/oidc/callback",
        "http://127.0.0.1:8740/auth/oidc/callback",
        "http://[::1]:8740/auth/oidc/callback",
    ] {
        assert_eq!(RedirectUri::try_from(text).unwrap().as_str(), text);
    }
}

#[test]
fn a_redirect_uri_over_remote_http_or_with_a_fragment_is_refused() {
    for text in [
        "http://hangar.example.net/auth/oidc/callback",
        "https://hangar.example.net/callback#x",
        "/auth/oidc/callback",
    ] {
        assert_eq!(
            RedirectUri::try_from(text),
            Err(ConfigError::RedirectUri),
            "{text}"
        );
    }
}

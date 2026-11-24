//! Unit tests of the issuer comparison, alone and through `check`.

use super::*;
use crate::allow::{Admit, Reach};
use crate::config::{ClientId, IssuerUrl, RedirectUri};
use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::rsa::{KeyPair, KeySize};
use aws_lc_rs::signature::{KeyPair as _, RSA_PKCS1_SHA256};
use serde_json::json;
use std::time::{Duration, UNIX_EPOCH};

#[test]
fn google_tokens_may_name_the_issuer_with_or_without_its_scheme() {
    let google = "https://accounts.google.com";
    assert!(issuer_matches(google, "https://accounts.google.com"));
    assert!(issuer_matches(google, "accounts.google.com"));
    assert!(!issuer_matches(google, "accounts.google.com.evil.example"));
    assert!(!issuer_matches(google, "http://accounts.google.com"));
}

#[test]
fn another_issuer_must_be_named_exactly() {
    let other = "https://id.example.net";
    assert!(issuer_matches(other, "https://id.example.net"));
    assert!(!issuer_matches(other, "id.example.net"));
    assert!(!issuer_matches(other, "https://id.example.net/"));
}

/// A config whose issuer is `issuer`, for the client `client`.
fn config_for(issuer: &str) -> OidcConfig {
    OidcConfig::new(
        IssuerUrl::try_from(issuer).unwrap(),
        ClientId::try_from("client").unwrap(),
        RedirectUri::try_from("http://localhost:8740/auth/oidc/callback").unwrap(),
        Admit::Anyone(Reach::Everything),
    )
}

/// A new RSA key, and the decoding key of its public half.
fn rsa_key() -> (KeyPair, DecodingKey) {
    let key = KeyPair::generate(KeySize::Rsa2048).unwrap();
    let public = key.public_key();
    let decoding = DecodingKey::from_rsa_components(
        &URL_SAFE_NO_PAD.encode(public.modulus().big_endian_without_leading_zero()),
        &URL_SAFE_NO_PAD.encode(public.exponent().big_endian_without_leading_zero()),
    )
    .unwrap();
    (key, decoding)
}

/// An RS256 token signed with `key` for the client `client`, with the nonce
/// `nonce` and the `iss` claim `iss`.
fn token_from(key: &KeyPair, iss: &str) -> SecretString {
    let header = json!({ "alg": "RS256", "kid": "key-1", "typ": "JWT" });
    let claims = json!({
        "iss": iss,
        "aud": "client",
        "exp": 1_900_000_000_u64,
        "nonce": "nonce",
        "email": "ada@example.com",
        "email_verified": true,
    });
    let signed = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    let mut signature = vec![0; key.public_modulus_len()];
    key.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        signed.as_bytes(),
        &mut signature,
    )
    .unwrap();
    SecretString::from(format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature)))
}

fn check_issuer(configured: &str, iss: &str) -> Result<Verified, SignInRefusal> {
    let (key, decoding) = rsa_key();
    let config = config_for(configured);
    let nonce = SecretString::from("nonce");
    let expected = Expected {
        config: &config,
        nonce: &nonce,
        now: UNIX_EPOCH + Duration::from_secs(1_800_000_000),
    };
    check(&token_from(&key, iss), &decoding, &expected)
}

#[test]
fn check_accepts_a_token_from_accounts_google_com_when_the_issuer_is_https_accounts_google_com() {
    let verified = check_issuer("https://accounts.google.com", "accounts.google.com");
    assert_eq!(
        verified.map(|verified| verified.email).ok(),
        Some("ada@example.com".to_owned())
    );
}

#[test]
fn check_refuses_a_token_whose_issuer_differs_from_the_configured_one() {
    for iss in ["https://id.example.net", "accounts.google.com.evil.example"] {
        let refused = check_issuer("https://accounts.google.com", iss).err();
        assert_eq!(
            refused,
            Some(SignInRefusal::TokenRejected(TokenFault::Issuer))
        );
    }
    let refused = check_issuer("https://id.example.net", "id.example.net").err();
    assert_eq!(
        refused,
        Some(SignInRefusal::TokenRejected(TokenFault::Issuer))
    );
}

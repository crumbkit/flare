//! Unit tests of the issuer comparison.

use super::*;

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

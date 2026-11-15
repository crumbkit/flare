//! The probe: a server proves it holds the token of a server file without
//! sending the token.
#![cfg(test)]

#[path = "support/fixture.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the fixture is test support: it panics on a broken fixture, and each test file uses part of it"
)]
pub mod fixture;

use fixture::loopback;
use hangar_auth_loopback::{Nonce, NonceError};

#[test]
fn a_new_nonce_is_32_hex_digits() {
    let nonce = Nonce::new().unwrap();
    assert_eq!(nonce.as_str().len(), 32);
    assert!(Nonce::try_from(nonce.as_str()).is_ok());
}

#[test]
fn a_nonce_of_16_and_128_hex_digits_passes() {
    assert!(Nonce::try_from("a".repeat(16).as_str()).is_ok());
    assert!(Nonce::try_from("a".repeat(128).as_str()).is_ok());
}

#[test]
fn a_nonce_of_15_or_129_digits_or_with_a_non_hex_character_is_refused() {
    for nonce in [
        "a".repeat(15),
        "a".repeat(129),
        format!("{}g", "a".repeat(31)),
    ] {
        assert_eq!(
            Nonce::try_from(nonce.as_str()).unwrap_err(),
            NonceError::Malformed,
            "{nonce}"
        );
    }
}

#[test]
fn the_proof_of_the_server_matches_its_own_record() {
    let loopback = loopback();
    let nonce = Nonce::new().unwrap();
    let proof = loopback.prove(&nonce);
    assert_eq!(proof.len(), 64);
    assert!(loopback.server_record().proof_matches(&nonce, &proof));
}

#[test]
fn a_proof_made_with_another_token_does_not_match() {
    let nonce = Nonce::new().unwrap();
    let proof = loopback().prove(&nonce);
    assert!(!loopback().server_record().proof_matches(&nonce, &proof));
}

#[test]
fn a_proof_for_another_nonce_does_not_match() {
    let loopback = loopback();
    let proof = loopback.prove(&Nonce::new().unwrap());
    assert!(
        !loopback
            .server_record()
            .proof_matches(&Nonce::new().unwrap(), &proof)
    );
}

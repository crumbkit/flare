//! Unit tests of the callback query parser, the PKCE challenge and the
//! sealed kept state.

use super::*;
use std::collections::HashSet;
use std::time::{Duration, UNIX_EPOCH};

const CLIENT: &str = "hangar-test-client";

fn sealer(key: u8, client: &str) -> Sealer {
    Sealer::from_bytes(&[key; 32], &ClientId::try_from(client).unwrap()).unwrap()
}

fn pending(return_to: &str, issued: SystemTime) -> Pending {
    Pending::new(ReturnTo::try_from(return_to).unwrap(), issued).unwrap()
}

/// A time with no fraction of a second, so whole-second arithmetic in the
/// tests is exact.
fn whole_second() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_800_000_000)
}

fn state_of(kept: &KeptState, sealer: &Sealer, now: SystemTime) -> String {
    sealer
        .open(kept, now)
        .map(|opened| opened.state.expose_secret().to_owned())
        .unwrap()
}

#[test]
fn the_pkce_challenge_matches_the_rfc_7636_appendix_b_vector() {
    assert_eq!(
        challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn a_callback_with_a_state_and_a_code_is_admitted() {
    let callback = Callback::parse("state=abc&code=4%2F0xyz&scope=email").unwrap();
    assert_eq!(callback.state, "abc");
    assert!(matches!(callback.outcome, Outcome::Code(_)));
    let denied = Callback::parse("error=access_denied&state=abc").unwrap();
    assert!(matches!(denied.outcome, Outcome::Denied));
}

#[test]
fn a_callback_without_a_state_with_two_codes_with_both_outcomes_or_too_long_is_malformed() {
    let long = format!("state=abc&code={}", "x".repeat(4096));
    for query in [
        "code=xyz",
        "state=abc",
        "state=abc&code=1&code=2",
        "state=a&state=b&code=1",
        "state=abc&code=1&error=access_denied",
        "state=&code=1",
        long.as_str(),
    ] {
        assert_eq!(
            Callback::parse(query).map(|_| ()),
            Err(SignInRefusal::Malformed),
            "{query}"
        );
    }
}

#[test]
fn a_return_to_value_of_up_to_2048_bytes_is_admitted_and_a_longer_one_refused() {
    assert!(ReturnTo::try_from("x".repeat(2048).as_str()).is_ok());
    assert_eq!(
        ReturnTo::try_from("x".repeat(2049).as_str()),
        Err(ConfigError::ReturnTo)
    );
}

#[test]
fn a_sealed_kept_state_opens_with_the_key_that_sealed_it() {
    let now = whole_second();
    let sealer = sealer(7, CLIENT);
    let made = pending("/pages/plan", now);
    let state = made.state.expose_secret().to_owned();
    let kept = sealer.seal(&made).unwrap();
    let opened = sealer.open(&kept, now).unwrap();
    assert_eq!(opened.state.expose_secret(), state);
    assert_eq!(opened.nonce.expose_secret(), made.nonce.expose_secret());
    assert_eq!(
        opened.verifier.expose_secret(),
        made.verifier.expose_secret()
    );
    assert_eq!(opened.return_to.as_str(), "/pages/plan");
    assert!(opened.matches(&state));
    assert!(!opened.matches("another-state"));
}

#[test]
fn a_sealed_kept_state_does_not_open_with_another_key_and_opens_with_its_own() {
    let now = whole_second();
    let kept = sealer(7, CLIENT).seal(&pending("/", now)).unwrap();
    assert_eq!(
        sealer(8, CLIENT).open(&kept, now).map(|_| ()),
        Err(SignInRefusal::UnknownState)
    );
    assert!(sealer(7, CLIENT).open(&kept, now).is_ok());
}

#[test]
fn a_sealed_kept_state_with_one_byte_changed_does_not_open_and_the_unchanged_one_does() {
    let now = whole_second();
    let sealer = sealer(7, CLIENT);
    let kept = sealer.seal(&pending("/", now)).unwrap();
    let text = kept.expose().to_owned();
    for at in [0, 12, text.len() / 2, text.len() - 2] {
        let mut bytes = text.clone().into_bytes();
        bytes[at] = if bytes[at] == b'A' { b'B' } else { b'A' };
        let changed = KeptState::from_cookie(std::str::from_utf8(&bytes).unwrap());
        assert_eq!(
            sealer.open(&changed, now).map(|_| ()),
            Err(SignInRefusal::UnknownState),
            "byte {at}"
        );
    }
    for garbage in ["", "!!!", "AAAA"] {
        assert_eq!(
            sealer
                .open(&KeptState::from_cookie(garbage), now)
                .map(|_| ()),
            Err(SignInRefusal::UnknownState),
            "{garbage}"
        );
    }
    assert!(sealer.open(&KeptState::from_cookie(&text), now).is_ok());
}

#[test]
fn a_sealed_kept_state_does_not_open_for_another_client_id_and_opens_for_its_own() {
    let now = whole_second();
    let kept = sealer(7, CLIENT).seal(&pending("/", now)).unwrap();
    assert_eq!(
        sealer(7, "another-client").open(&kept, now).map(|_| ()),
        Err(SignInRefusal::UnknownState)
    );
    assert!(sealer(7, CLIENT).open(&kept, now).is_ok());
}

#[test]
fn a_sealed_kept_state_opens_for_just_under_ten_minutes_and_not_after() {
    let issued = whole_second();
    let sealer = sealer(7, CLIENT);
    let kept = sealer.seal(&pending("/", issued)).unwrap();
    assert!(
        sealer
            .open(&kept, issued + Duration::from_secs(599))
            .is_ok()
    );
    assert_eq!(
        sealer
            .open(&kept, issued + Duration::from_secs(600))
            .map(|_| ()),
        Err(SignInRefusal::UnknownState)
    );
}

#[test]
fn a_sealed_kept_state_issued_more_than_sixty_seconds_ahead_does_not_open_and_sixty_does() {
    let now = whole_second();
    let sealer = sealer(7, CLIENT);
    let ahead = sealer
        .seal(&pending("/", now + Duration::from_secs(61)))
        .unwrap();
    assert_eq!(
        sealer.open(&ahead, now).map(|_| ()),
        Err(SignInRefusal::UnknownState)
    );
    let just = sealer
        .seal(&pending("/", now + Duration::from_secs(60)))
        .unwrap();
    assert!(sealer.open(&just, now).is_ok());
}

#[test]
fn a_2048_byte_return_path_seals_to_a_cookie_value_under_3100_bytes() {
    let now = whole_second();
    let long = format!("/{}", "x".repeat(2047));
    let kept = sealer(7, CLIENT).seal(&pending(&long, now)).unwrap();
    assert!(kept.expose().len() < 3100, "{}", kept.expose().len());
    assert!(
        kept.expose()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    );
    assert_eq!(state_of(&kept, &sealer(7, CLIENT), now).len(), 43);
}

#[test]
fn a_thousand_seals_use_a_thousand_distinct_nonces() {
    let now = whole_second();
    let sealer = sealer(7, CLIENT);
    let made = pending("/", now);
    let mut nonces = HashSet::new();
    for _ in 0..1000 {
        let kept = sealer.seal(&made).unwrap();
        let bytes = URL_SAFE_NO_PAD.decode(kept.expose()).unwrap();
        nonces.insert(bytes[..12].to_vec());
    }
    assert_eq!(nonces.len(), 1000);
}

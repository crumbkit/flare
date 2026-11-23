//! Unit tests of the callback query parser, the PKCE challenge.

use super::*;

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

use super::*;
use crate::Email;

fn owner() -> Email {
    Email::try_from("ana@example.org").unwrap()
}

#[test]
fn a_token_id_parses_only_32_lower_case_hexadecimal_characters() {
    let id = TokenId::from_random([0xab; 16]);
    assert_eq!(TokenId::try_from(id.as_str()).unwrap(), id);
    for bad in ["", "AB", &"A".repeat(32), &"g".repeat(32), &"a".repeat(33)] {
        assert!(TokenId::try_from(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_label_admits_64_visible_bytes_and_refuses_an_empty_long_or_control_label() {
    assert!(TokenLabel::try_from("flare-cli on studio").is_ok());
    assert!(TokenLabel::try_from("x".repeat(64).as_str()).is_ok());
    for bad in ["", &"x".repeat(65), "tab\there", "new\nline"] {
        assert!(TokenLabel::try_from(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn the_debug_output_of_a_digest_and_a_stored_token_holds_no_digest_or_address() {
    let digest = Digest::from_bytes([0x5a; 32]);
    assert!(!format!("{digest:?}").contains("5a"));
    let token = NewToken::new(
        TokenId::from_random([1; 16]),
        digest,
        owner(),
        TokenLabel::try_from("cli").unwrap(),
        Timestamp::from_secs(10),
    );
    let shown = format!("{:?}", token.stored());
    assert!(!shown.contains("ana@example.org"));
    assert!(!shown.contains("5a5a"));
}

#[test]
fn a_token_lives_90_days_and_a_revoked_one_is_never_active() {
    let token = NewToken::new(
        TokenId::from_random([1; 16]),
        Digest::from_bytes([0; 32]),
        owner(),
        TokenLabel::try_from("cli").unwrap(),
        Timestamp::from_secs(100),
    )
    .stored();
    assert_eq!(token.expires().as_secs(), 100 + 90 * 24 * 60 * 60);
    assert!(token.is_active(Timestamp::from_secs(100)));
    assert!(!token.is_active(token.expires()));
    let revoked = token.revoked_at(Timestamp::from_secs(200));
    assert!(!revoked.is_active(Timestamp::from_secs(150)));
}

#[test]
fn a_login_code_expires_60_seconds_after_issue() {
    let code = LoginCode::new(
        Digest::from_bytes([2; 32]),
        owner(),
        4567,
        Timestamp::from_secs(1_000),
    );
    assert_eq!(code.expires().as_secs(), 1_060);
    assert!(code.is_live(Timestamp::from_secs(1_059)));
    assert!(!code.is_live(Timestamp::from_secs(1_060)));
}

#[test]
fn a_used_code_built_from_its_parts_equals_the_one_its_login_code_gives() {
    let code = LoginCode::new(
        Digest::from_bytes([1; 32]),
        owner(),
        4321,
        Timestamp::from_secs(5),
    );
    assert_eq!(UsedCode::new(owner(), 4321), code.used());
}

#[test]
fn a_digest_reads_back_from_its_hex_and_refuses_other_text() {
    let digest = Digest::from_bytes([0xab; 32]);
    assert_eq!(Digest::from_hex(&digest.to_hex()), Some(digest));
    let mixed = Digest::from_bytes(core::array::from_fn(|index| u8::try_from(index).unwrap()));
    assert_eq!(Digest::from_hex(&mixed.to_hex()), Some(mixed));
    for bad in [
        "",
        "AB",
        &"A".repeat(64),
        &"g".repeat(64),
        &"a".repeat(63),
        &"a".repeat(66),
    ] {
        assert_eq!(Digest::from_hex(bad), None, "{bad}");
    }
}

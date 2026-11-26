use super::first_line;
use secrecy::ExposeSecret as _;

#[test]
fn the_first_line_without_its_end_is_the_token() {
    let token = first_line(b"abc.def-ghi\r\nsecond line\n").unwrap();
    assert_eq!(token.expose_secret(), "abc.def-ghi");
}

#[test]
fn an_empty_first_line_is_refused_and_a_token_with_no_line_end_is_admitted() {
    assert!(first_line(b"").is_err());
    assert!(first_line(b"\nabc\n").is_err());
    assert_eq!(first_line(b"abc").unwrap().expose_secret(), "abc");
}

#[test]
fn a_first_line_with_a_space_or_a_control_character_is_refused() {
    assert!(first_line(b"two words\n").is_err());
    assert!(first_line(b"tab\tbed\n").is_err());
    assert!(first_line("caf\u{e9}\n".as_bytes()).is_err());
    assert!(first_line(b"one-token\n").is_ok());
}

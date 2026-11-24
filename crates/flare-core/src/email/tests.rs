//! Unit tests of [`Email`].

use super::Email;
use crate::error::{ParseError, Subject};
use crate::name::PrincipalId;

fn refused(text: &str) -> bool {
    matches!(
        Email::try_from(text),
        Err(ParseError::Malformed {
            subject: Subject::Email,
            ..
        } | ParseError::TooLong {
            subject: Subject::Email,
            ..
        } | ParseError::Empty {
            subject: Subject::Email
        })
    )
}

#[test]
fn an_email_is_held_in_lower_case() {
    let email = Email::try_from("Alice@Example.COM").unwrap();
    assert_eq!(email.as_str(), "alice@example.com");
    assert_eq!(email, Email::try_from("alice@example.com").unwrap());
}

#[test]
fn an_email_without_one_at_sign_between_two_parts_or_with_spaces_is_refused() {
    for text in [
        "",
        "alice",
        "@example.com",
        "alice@",
        "a@b@c",
        "alice @example.com",
        "alice@example.com ",
        " alice@example.com",
        "alice@exa\nmple.com",
    ] {
        assert!(refused(text), "{text:?}");
    }
    assert!(Email::try_from("alice@example.com").is_ok());
}

#[test]
fn an_email_of_254_bytes_is_admitted_and_one_of_255_is_refused() {
    let longest = format!("{}@example.com", "a".repeat(254 - "@example.com".len()));
    assert_eq!(longest.len(), 254);
    assert!(Email::try_from(longest.as_str()).is_ok());
    let longer = format!("a{longest}");
    assert!(refused(&longer));
}

#[test]
fn an_email_with_a_cyrillic_look_alike_a_fullwidth_letter_or_the_kelvin_sign_is_refused() {
    for text in [
        "\u{0430}lice@example.com",
        "\u{FF41}lice@example.com",
        "\u{212A}ate@example.com",
        "kate@\u{212A}.example.com",
        "j\u{00F6}rg@example.com",
    ] {
        assert!(refused(text), "{text:?}");
    }
    assert_eq!(
        Email::try_from("KATE@example.com").unwrap().as_str(),
        "kate@example.com"
    );
}

#[test]
fn dots_and_plus_signs_stay_significant_so_three_spellings_are_three_addresses() {
    let dotted = Email::try_from("a.lice@x.com").unwrap();
    let plain = Email::try_from("alice@x.com").unwrap();
    let tagged = Email::try_from("alice+t@x.com").unwrap();
    assert_ne!(dotted, plain);
    assert_ne!(plain, tagged);
    assert_ne!(dotted, tagged);
}

#[test]
fn the_debug_output_of_an_email_hides_the_address() {
    let email = Email::try_from("alice@example.com").unwrap();
    let debug = format!("{email:?}");
    assert!(!debug.contains("alice"), "{debug}");
    assert!(!debug.contains("example"), "{debug}");
}

#[test]
fn the_principal_id_of_an_email_is_the_address_in_lower_case() {
    let email = Email::try_from("Alice@Example.com").unwrap();
    assert_eq!(
        email.principal_id(),
        PrincipalId::try_from("alice@example.com").unwrap()
    );
}

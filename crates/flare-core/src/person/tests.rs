//! Unit tests of [`Username`], [`Person`] and [`IdentityKind`].

use super::{IdentityKind, Person, Username};
use crate::email::Email;
use crate::error::{ParseError, Subject};

fn refused(text: &str) -> bool {
    matches!(
        Username::try_from(text),
        Err(ParseError::Malformed {
            subject: Subject::Username,
            ..
        } | ParseError::TooLong {
            subject: Subject::Username,
            ..
        } | ParseError::Empty {
            subject: Subject::Username
        })
    )
}

#[test]
fn a_username_is_held_in_lower_case() {
    let name = Username::try_from("Ana").unwrap();
    assert_eq!(name.as_str(), "ana");
    assert_eq!(name, Username::try_from("ana").unwrap());
}

#[test]
fn a_username_of_letters_digits_dots_underscores_and_dashes_is_admitted() {
    for text in [
        "ana", "a", "0ana", "ana.lee", "ana_lee", "ana-lee", "a1.b_c-d",
    ] {
        assert!(Username::try_from(text).is_ok(), "{text}");
    }
}

#[test]
fn a_username_with_an_at_sign_a_colon_or_a_reserved_word_is_refused() {
    for text in [
        "",
        "ana@example.org",
        "team:design",
        "everyone",
        "Everyone",
        "me",
        "ME",
        ".ana",
        "_ana",
        "-ana",
        "ana lee",
        "anä",
    ] {
        assert!(refused(text), "{text:?}");
    }
}

#[test]
fn a_username_of_64_bytes_is_admitted_and_one_of_65_is_refused() {
    assert!(Username::try_from("a".repeat(64).as_str()).is_ok());
    assert!(matches!(
        Username::try_from("a".repeat(65).as_str()),
        Err(ParseError::TooLong {
            subject: Subject::Username,
            max: 64
        })
    ));
}

#[test]
fn a_person_with_an_at_sign_is_an_address_and_one_without_is_a_username() {
    let address = Person::try_from("Ana@Example.org").unwrap();
    assert_eq!(address.kind(), IdentityKind::Email);
    assert_eq!(address.as_str(), "ana@example.org");
    assert_eq!(
        address,
        Person::from(Email::try_from("ana@example.org").unwrap())
    );
    let name = Person::try_from("Ana").unwrap();
    assert_eq!(name.kind(), IdentityKind::Username);
    assert_eq!(name.as_str(), "ana");
    assert_eq!(name, Person::from(Username::try_from("ana").unwrap()));
}

#[test]
fn a_person_round_trips_through_its_principal_id() {
    for text in ["ana@example.org", "ana"] {
        let person = Person::try_from(text).unwrap();
        let id = person.principal_id();
        assert_eq!(id.as_str(), text);
        assert_eq!(Person::try_from(id.as_str()).unwrap(), person);
    }
}

#[test]
fn a_stored_id_that_is_neither_kind_is_refused() {
    for text in ["", "everyone", "team:design", "a@b@c", "me"] {
        assert!(Person::try_from(text).is_err(), "{text:?}");
    }
}

#[test]
fn an_identity_kind_reads_only_its_own_kind() {
    assert!(IdentityKind::Email.person("ana@example.org").is_ok());
    assert!(IdentityKind::Email.person("ana").is_err());
    assert!(IdentityKind::Username.person("ana").is_ok());
    assert!(IdentityKind::Username.person("ana@example.org").is_err());
    assert_eq!(IdentityKind::default(), IdentityKind::Email);
}

#[test]
fn an_identity_kind_is_written_as_email_or_username() {
    assert_eq!(IdentityKind::Email.as_str(), "email");
    assert_eq!(IdentityKind::Username.as_str(), "username");
    assert_eq!(IdentityKind::try_from("email"), Ok(IdentityKind::Email));
    assert_eq!(
        IdentityKind::try_from("username"),
        Ok(IdentityKind::Username)
    );
    assert!(IdentityKind::try_from("Email").is_err());
}

#[test]
fn a_person_hides_its_value_in_debug_output() {
    let person = Person::try_from("ana").unwrap();
    assert!(!format!("{person:?}").contains("ana"));
}

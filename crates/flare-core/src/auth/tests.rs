//! Unit tests of credentials and principals.

use super::*;

#[test]
fn a_credential_debug_output_hides_the_secret() {
    for credential in [
        Credential::bearer("hunter2-secret-token"),
        Credential::session("hunter2-secret-token"),
    ] {
        let shown = format!("{credential:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
    }
}

#[test]
fn a_credential_remembers_which_kind_it_is() {
    assert!(matches!(Credential::bearer("x"), Credential::Bearer(_)));
    assert!(matches!(Credential::session("x"), Credential::Session(_)));
}

#[test]
fn a_principal_exposes_its_id() {
    let id = PrincipalId::try_from("ada").unwrap();
    assert_eq!(Principal::new(id.clone()).id(), &id);
}

#[test]
fn a_principal_built_with_a_verified_email_keeps_it_and_one_built_from_an_id_has_none() {
    let id = PrincipalId::try_from("bob@y.com").unwrap();
    let email = crate::Email::try_from("mallory@x.com").unwrap();
    let principal = Principal::with_email(id.clone(), email.clone());
    assert_eq!(principal.id(), &id);
    assert_eq!(principal.email(), Some(&email));
    assert_eq!(Principal::new(id).email(), None);
    let shown = format!("{principal:?}");
    assert!(!shown.contains("mallory"), "{shown}");
}

#[test]
fn a_principal_built_with_an_email_is_the_person_with_that_address() {
    let email = crate::Email::try_from("bob@y.com").unwrap();
    let principal = Principal::with_email(email.principal_id(), email.clone());
    assert_eq!(principal.person(), Some(&crate::Person::Email(email)));
}

#[test]
fn a_principal_built_with_a_username_keeps_its_verified_email_beside_it() {
    let name = crate::Person::try_from("bob").unwrap();
    let email = crate::Email::try_from("bob@y.com").unwrap();
    let id = PrincipalId::try_from("bob").unwrap();
    let principal = Principal::with_person(id.clone(), name.clone(), Some(email.clone()));
    assert_eq!(principal.person(), Some(&name));
    assert_eq!(principal.email(), Some(&email));
    let bare = Principal::with_person(id, name.clone(), None);
    assert_eq!(bare.person(), Some(&name));
    assert_eq!(bare.email(), None);
}

#[test]
fn a_principal_built_from_an_id_has_no_person() {
    let id = PrincipalId::try_from("ada").unwrap();
    assert_eq!(Principal::new(id).person(), None);
}

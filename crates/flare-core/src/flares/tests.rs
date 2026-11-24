use super::*;
use crate::{Email, Principal, PrincipalId};

fn person(address: &str) -> Principal {
    Principal::with_email(
        PrincipalId::try_from(address).unwrap(),
        Email::try_from(address).unwrap(),
    )
}

#[test]
fn a_creators_list_admits_its_addresses_and_refuses_everyone_else() {
    let creators = Creators::only(vec![Email::try_from("Ana@Example.org").unwrap()]);
    assert!(creators.admits(&person("ana@example.org")));
    assert!(!creators.admits(&person("jo@example.org")));
}

#[test]
fn creators_everyone_admits_any_verified_address_and_no_principal_without_one() {
    assert!(Creators::Everyone.admits(&person("jo@example.org")));
    let local = Principal::new(PrincipalId::try_from("local:jo").unwrap());
    assert!(!Creators::Everyone.admits(&local));
}

#[test]
fn the_address_only_recipients_accept_an_address_and_refuse_a_name() {
    let recipients = AddressesOnly;
    let ana = crate::contract::block_on(recipients.resolve("ana@example.org"));
    assert_eq!(ana.unwrap().as_str(), "ana@example.org");
    let name = crate::contract::block_on(recipients.resolve("ana"));
    assert_eq!(name.unwrap_err(), RecipientError::NotAnAddress);
}

fn domain(text: &str) -> CreatorDomain {
    CreatorDomain::try_from(text).unwrap()
}

#[test]
fn a_creator_domain_admits_its_addresses_and_refuses_other_domains() {
    let creators = Creators::listed(vec![], vec![domain("@Example.com")]);
    assert!(creators.admits(&person("ana@example.com")));
    assert!(!creators.admits(&person("ana@other.example")));
}

#[test]
fn a_creator_domain_does_not_admit_a_subdomain_or_a_longer_domain() {
    let creators = Creators::listed(vec![], vec![domain("@example.com")]);
    assert!(!creators.admits(&person("ana@mail.example.com")));
    assert!(!creators.admits(&person("ana@notexample.com")));
}

#[test]
fn a_list_of_addresses_and_domains_admits_either() {
    let creators = Creators::listed(
        vec![Email::try_from("jo@other.example").unwrap()],
        vec![domain("@example.com")],
    );
    assert!(creators.admits(&person("ana@example.com")));
    assert!(creators.admits(&person("jo@other.example")));
    assert!(!creators.admits(&person("sam@other.example")));
}

#[test]
fn a_creator_domain_refuses_a_principal_without_an_address() {
    let creators = Creators::listed(vec![], vec![domain("@example.com")]);
    let local = Principal::new(PrincipalId::try_from("local:ana").unwrap());
    assert!(!creators.admits(&local));
}

#[test]
fn a_creator_domain_entry_must_be_an_at_sign_and_a_plain_domain() {
    for text in [
        "",
        "@",
        "example.com",
        "ana@example.com",
        "@*.example.com",
        "@exa mple.com",
        "@a@b",
    ] {
        assert!(CreatorDomain::try_from(text).is_err(), "{text:?}");
    }
}

#[test]
fn a_configured_name_resolves_to_its_address_and_an_address_still_resolves() {
    let ana = Email::try_from("ana@example.com").unwrap();
    let recipients = NamedRecipients::new([("ana".to_owned(), ana)]);
    let named = crate::contract::block_on(recipients.resolve("ana"));
    assert_eq!(named.unwrap().as_str(), "ana@example.com");
    let full = crate::contract::block_on(recipients.resolve("Jo@Example.org"));
    assert_eq!(full.unwrap().as_str(), "jo@example.org");
}

#[test]
fn a_name_is_matched_in_lower_case() {
    let ana = Email::try_from("ana@example.com").unwrap();
    let recipients = NamedRecipients::new([("ana".to_owned(), ana)]);
    let named = crate::contract::block_on(recipients.resolve("Ana"));
    assert_eq!(named.unwrap().as_str(), "ana@example.com");
}

#[test]
fn an_unknown_name_is_refused_as_not_an_address() {
    let ana = Email::try_from("ana@example.com").unwrap();
    let recipients = NamedRecipients::new([("ana".to_owned(), ana)]);
    let unknown = crate::contract::block_on(recipients.resolve("sam"));
    assert_eq!(unknown.unwrap_err(), RecipientError::NotAnAddress);
}

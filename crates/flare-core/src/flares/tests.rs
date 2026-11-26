use super::*;
use crate::{Email, IdentityKind, Person, Principal, PrincipalId};

fn person(address: &str) -> Principal {
    Principal::with_email(
        PrincipalId::try_from(address).unwrap(),
        Email::try_from(address).unwrap(),
    )
}

fn may_create(creators: &impl Creators, who: &Principal) -> bool {
    crate::contract::block_on(creators.may_create(who)).unwrap()
}

#[test]
fn a_creators_list_admits_its_addresses_and_refuses_everyone_else() {
    let creators = CreatorList::only(vec![Person::try_from("Ana@Example.org").unwrap()]);
    assert!(may_create(&creators, &person("ana@example.org")));
    assert!(!may_create(&creators, &person("jo@example.org")));
}

#[test]
fn all_admitted_lets_any_principal_it_is_asked_about_create() {
    for address in [
        "jo@example.org",
        "ana@other.example",
        "sam@mail.example.com",
    ] {
        assert!(may_create(&AllAdmitted, &person(address)));
    }
}

#[test]
fn the_email_recipients_accept_an_address_and_refuse_a_name() {
    let recipients = ByIdentity(IdentityKind::Email);
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
    let creators = CreatorList::listed(vec![], vec![domain("@Example.com")]);
    assert!(may_create(&creators, &person("ana@example.com")));
    assert!(!may_create(&creators, &person("ana@other.example")));
}

#[test]
fn a_creator_domain_does_not_admit_a_subdomain_or_a_longer_domain() {
    let creators = CreatorList::listed(vec![], vec![domain("@example.com")]);
    assert!(!may_create(&creators, &person("ana@mail.example.com")));
    assert!(!may_create(&creators, &person("ana@notexample.com")));
}

#[test]
fn a_list_of_addresses_and_domains_admits_either() {
    let creators = CreatorList::listed(
        vec![Person::try_from("jo@other.example").unwrap()],
        vec![domain("@example.com")],
    );
    assert!(may_create(&creators, &person("ana@example.com")));
    assert!(may_create(&creators, &person("jo@other.example")));
    assert!(!may_create(&creators, &person("sam@other.example")));
}

#[test]
fn a_creators_list_refuses_a_principal_without_an_address() {
    let creators = CreatorList::listed(vec![], vec![domain("@example.com")]);
    let local = Principal::new(PrincipalId::try_from("local:ana").unwrap());
    assert!(!may_create(&creators, &local));
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
    let ana = Person::try_from("ana@example.com").unwrap();
    let recipients = NamedRecipients::new([("ana".to_owned(), ana)]);
    let named = crate::contract::block_on(recipients.resolve("ana"));
    assert_eq!(named.unwrap().as_str(), "ana@example.com");
    let full = crate::contract::block_on(recipients.resolve("Jo@Example.org"));
    assert_eq!(full.unwrap().as_str(), "jo@example.org");
}

#[test]
fn a_name_is_matched_in_lower_case() {
    let ana = Person::try_from("ana@example.com").unwrap();
    let recipients = NamedRecipients::new([("ana".to_owned(), ana)]);
    let named = crate::contract::block_on(recipients.resolve("Ana"));
    assert_eq!(named.unwrap().as_str(), "ana@example.com");
}

#[test]
fn an_unknown_name_is_refused_as_not_an_address() {
    let ana = Person::try_from("ana@example.com").unwrap();
    let recipients = NamedRecipients::new([("ana".to_owned(), ana)]);
    let unknown = crate::contract::block_on(recipients.resolve("sam"));
    assert_eq!(unknown.unwrap_err(), RecipientError::NotAnAddress);
}

#[test]
fn a_creators_list_admits_a_listed_username_and_a_username_whose_verified_email_is_at_a_domain() {
    let creators = CreatorList::listed(
        vec![Person::try_from("jo").unwrap()],
        vec![domain("@example.com")],
    );
    let user = |name: &str, email: Option<&str>| {
        Principal::with_person(
            PrincipalId::try_from(name).unwrap(),
            Person::try_from(name).unwrap(),
            email.map(|text| Email::try_from(text).unwrap()),
        )
    };
    assert!(may_create(&creators, &user("jo", None)));
    assert!(may_create(&creators, &user("ana", Some("ana@example.com"))));
    assert!(!may_create(
        &creators,
        &user("sam", Some("sam@other.example"))
    ));
    assert!(!may_create(&creators, &user("eli", None)));
}

#[test]
fn named_recipients_under_the_username_kind_resolve_a_name_and_refuse_an_address() {
    let jo = Person::try_from("jo").unwrap();
    let recipients =
        NamedRecipients::new([("joanna".to_owned(), jo)]).with_identity(IdentityKind::Username);
    let named = crate::contract::block_on(recipients.resolve("Joanna"));
    assert_eq!(named.unwrap().as_str(), "jo");
    let user = crate::contract::block_on(recipients.resolve("sam"));
    assert_eq!(user.unwrap().as_str(), "sam");
    let address = crate::contract::block_on(recipients.resolve("sam@example.org"));
    assert_eq!(address.unwrap_err(), RecipientError::NotAUsername);
}

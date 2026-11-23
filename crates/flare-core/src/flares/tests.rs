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

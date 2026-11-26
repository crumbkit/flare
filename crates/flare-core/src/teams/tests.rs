use super::*;
use crate::auth::Principal;
use crate::email::Email;
use crate::name::PrincipalId;

#[test]
fn a_team_name_takes_lowercase_letters_digits_and_dashes_up_to_forty_bytes() {
    assert!(TeamName::try_from("design-2").is_ok());
    assert!(TeamName::try_from("").is_err());
    assert!(TeamName::try_from("Design").is_err());
    assert!(TeamName::try_from("a b").is_err());
    assert!(TeamName::try_from("a:b").is_err());
    assert!(TeamName::try_from(&"a".repeat(40)[..]).is_ok());
    assert!(TeamName::try_from(&"a".repeat(41)[..]).is_err());
}

#[test]
fn a_group_id_round_trips_and_never_parses_as_a_person() {
    let team = Group::Team(TeamName::try_from("design").unwrap());
    assert_eq!(team.principal_id().as_str(), "team:design");
    assert_eq!(Group::of_grantee(&team.principal_id()), Some(team));
    assert_eq!(Group::Everyone.principal_id().as_str(), "everyone");
    let person = PrincipalId::try_from("ana@example.com").unwrap();
    assert_eq!(Group::of_grantee(&person), None);
}

#[test]
fn a_with_identifier_names_a_team_everyone_or_a_person() {
    assert_eq!(Group::parse("everyone"), Some(Ok(Group::Everyone)));
    assert!(matches!(
        Group::parse("team:design"),
        Some(Ok(Group::Team(_)))
    ));
    assert!(matches!(Group::parse("team:Bad Name"), Some(Err(_))));
    assert_eq!(Group::parse("ana@example.com"), None);
}

#[test]
fn an_audience_round_trips_as_a_domain_admitted_or_signed_in() {
    let domain = Audience::try_from("Example.COM").unwrap();
    assert_eq!(domain.as_str(), "example.com");
    assert_eq!(Audience::try_from("admitted"), Ok(Audience::Admitted));
    assert_eq!(Audience::try_from("signed-in"), Ok(Audience::SignedIn));
    assert_eq!(Audience::SignedIn.as_str(), "signed-in");
    assert!(Audience::try_from("nodot").is_err());
}

fn signed_in(address: &str) -> Principal {
    let email = Email::try_from(address).unwrap();
    Principal::with_email(email.principal_id(), email)
}

#[test]
fn an_anyone_rule_gives_no_audience_and_a_domain_audience_holds_only_its_domain() {
    assert_eq!(Admission::Anyone.audience(false), None);
    let domain = MailDomain::try_from("example.com").unwrap();
    let audience = Admission::Domain(domain).audience(false).unwrap();
    let inside = signed_in("ana@example.com");
    let below = signed_in("ana@mail.example.com");
    assert!(Admission::Anyone.opens(&audience, &inside, false));
    assert!(!Admission::Anyone.opens(&audience, &below, false));
    assert!(Admission::Listed.opens(&Audience::Admitted, &inside, false));
    assert!(!Admission::Anyone.opens(&Audience::Admitted, &inside, false));
}

#[test]
fn an_anyone_rule_with_everyone_signed_in_gives_the_signed_in_audience_which_holds_while_it_is_on()
{
    assert_eq!(Admission::Anyone.audience(true), Some(Audience::SignedIn));
    let domain = MailDomain::try_from("example.com").unwrap();
    assert_eq!(
        Admission::Domain(domain.clone()).audience(true),
        Some(Audience::Domain(domain))
    );
    assert_eq!(Admission::Listed.audience(true), Some(Audience::Admitted));
    let username = Principal::with_person(
        PrincipalId::try_from("ana").unwrap(),
        crate::Person::try_from("ana").unwrap(),
        None,
    );
    for who in [signed_in("ana@other.example"), username] {
        assert!(Admission::Anyone.opens(&Audience::SignedIn, &who, true));
        assert!(!Admission::Anyone.opens(&Audience::SignedIn, &who, false));
    }
}

use super::*;
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
fn an_audience_round_trips_as_a_domain_or_admitted() {
    let domain = Audience::try_from("Example.COM").unwrap();
    assert_eq!(domain.as_str(), "example.com");
    assert_eq!(Audience::try_from("admitted"), Ok(Audience::Admitted));
    assert!(Audience::try_from("nodot").is_err());
}

#[test]
fn an_anyone_rule_gives_no_audience_and_a_domain_audience_holds_only_its_domain() {
    assert_eq!(Admission::Anyone.audience(), None);
    let domain = MailDomain::try_from("example.com").unwrap();
    let audience = Admission::Domain(domain).audience().unwrap();
    let inside = Email::try_from("ana@example.com").unwrap();
    let below = Email::try_from("ana@mail.example.com").unwrap();
    assert!(Admission::Anyone.opens(&audience, &inside));
    assert!(!Admission::Anyone.opens(&audience, &below));
    assert!(Admission::Listed.opens(&Audience::Admitted, &inside));
    assert!(!Admission::Anyone.opens(&Audience::Admitted, &inside));
}

//! Unit tests of the allow-list, the domain rule and the principals they
//! build.

use super::*;
use flare_core::{Email, PageName};
use serde_json::json;

fn entry(email: &str, principal: &str) -> AllowEntry {
    AllowEntry::new(
        Email::try_from(email).unwrap(),
        PrincipalId::try_from(principal).unwrap(),
        Reach::Everything,
    )
}

#[test]
fn an_allow_list_refuses_one_email_or_one_principal_twice_and_admits_distinct_entries() {
    assert_eq!(
        AllowList::new(vec![entry("a@x.net", "a"), entry("A@x.net", "b")]).map(|_| ()),
        Err(ConfigError::DuplicateEmail)
    );
    assert_eq!(
        AllowList::new(vec![entry("a@x.net", "a"), entry("b@x.net", "a")]).map(|_| ()),
        Err(ConfigError::DuplicatePrincipal)
    );
    assert!(AllowList::new(vec![entry("a@x.net", "a"), entry("b@x.net", "b")]).is_ok());
}

#[test]
fn a_reach_of_only_some_resources_reaches_those_and_no_other() {
    let home = Resource::Page(PageName::try_from("home").unwrap());
    let other = Resource::Page(PageName::try_from("other").unwrap());
    let only = Reach::Only(vec![home.clone()]);
    assert!(only.reaches(&home));
    assert!(!only.reaches(&other));
    assert!(Reach::Everything.reaches(&other));
}

#[test]
fn a_workspace_domain_is_admitted_in_lower_case_whatever_case_it_is_written_in() {
    let lower = WorkspaceDomain::try_from("example.com").unwrap();
    let mixed = WorkspaceDomain::try_from("Example.COM").unwrap();
    assert_eq!(lower, mixed);
    assert_eq!(mixed.as_str(), "example.com");
    for text in ["eu.example.com", "a-b.example.co.uk", "x1.io"] {
        assert_eq!(WorkspaceDomain::try_from(text).unwrap().as_str(), text);
    }
    let longest_label = format!("{}.com", "a".repeat(63));
    assert!(WorkspaceDomain::try_from(longest_label.as_str()).is_ok());
    let longest_name = format!(
        "{}.{}.{}.{}",
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61)
    );
    assert_eq!(longest_name.len(), 253);
    assert!(WorkspaceDomain::try_from(longest_name.as_str()).is_ok());
}

#[test]
fn a_workspace_domain_that_is_not_a_dns_name_of_two_labels_is_refused() {
    let long_label = format!("{}.com", "a".repeat(64));
    let long_name = format!(
        "{}.{}.{}.{}",
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(62)
    );
    assert_eq!(long_name.len(), 254);
    for text in [
        "",
        "localhost",
        ".example.com",
        "example.com.",
        "example..com",
        "ada@example.com",
        "example .com",
        " example.com",
        "ex\u{00E4}mple.com",
        "-example.com",
        "example-.com",
        "exa_mple.com",
        long_label.as_str(),
        long_name.as_str(),
    ] {
        assert_eq!(
            WorkspaceDomain::try_from(text),
            Err(ConfigError::Domain),
            "{text:?}"
        );
    }
}

#[test]
fn an_allow_list_sign_in_builds_its_principal_with_the_verified_email_in_lower_case() {
    let allow = AllowList::new(vec![entry("ada@example.com", "google:ada")]).unwrap();
    let principal = Admit::Emails(allow)
        .principal("Ada@Example.com", None)
        .unwrap();
    assert_eq!(principal.id().as_str(), "google:ada");
    assert_eq!(
        principal.email(),
        Some(&Email::try_from("ada@example.com").unwrap())
    );
}

#[test]
fn an_entry_that_maps_one_address_to_another_address_shaped_principal_keeps_the_first_address() {
    let allow = AllowList::new(vec![entry("mallory@x.com", "bob@y.com")]).unwrap();
    let principal = Admit::Emails(allow)
        .principal("mallory@x.com", None)
        .unwrap();
    assert_eq!(principal.id().as_str(), "bob@y.com");
    assert_eq!(principal.email().map(Email::as_str), Some("mallory@x.com"));
    // The admitted twin: an address off the list is refused.
    let allow = AllowList::new(vec![entry("mallory@x.com", "bob@y.com")]).unwrap();
    assert!(Admit::Emails(allow).principal("bob@y.com", None).is_err());
}

#[test]
fn a_domain_sign_in_builds_its_principal_with_the_verified_email_in_lower_case() {
    let rule = DomainRule::new(
        WorkspaceDomain::try_from("example.com").unwrap(),
        Reach::Everything,
    );
    let hd = json!("example.com");
    let principal = Admit::Domain(rule)
        .principal("Ada@Example.com", Some(&hd))
        .unwrap();
    assert_eq!(principal.id().as_str(), "ada@example.com");
    assert_eq!(
        principal.email().map(Email::as_str),
        Some("ada@example.com")
    );
}

#[test]
fn the_anyone_rule_admits_any_verified_address_as_its_own_principal_and_refuses_a_malformed_one() {
    let admit = Admit::Anyone(Reach::Only(Vec::new()));
    let principal = admit.principal("Ola@Outside.Example", None).unwrap();
    assert_eq!(principal.id().as_str(), "ola@outside.example");
    assert_eq!(principal.email().unwrap().as_str(), "ola@outside.example");
    assert_eq!(
        admit.principal("not an address", None).unwrap_err(),
        SignInRefusal::NotAllowed
    );
    assert_eq!(admit.hosted_domain_hint(), None);
}

//! Unit tests of emails and the allow-list.

use super::*;
use hangar_core::PageName;

fn entry(email: &str, principal: &str) -> AllowEntry {
    AllowEntry::new(
        Email::try_from(email).unwrap(),
        PrincipalId::try_from(principal).unwrap(),
        Reach::Everything,
    )
}

#[test]
fn an_email_is_admitted_in_lower_case() {
    let email = Email::try_from("Ada.Lovelace@Example.COM").unwrap();
    assert_eq!(email.as_str(), "ada.lovelace@example.com");
}

#[test]
fn an_email_without_one_at_sign_or_with_spaces_is_refused() {
    for text in [
        "",
        "ada",
        "@example.com",
        "ada@",
        "a@b@c",
        "ada @example.com",
        "ada@exa\nmple.com",
    ] {
        assert_eq!(Email::try_from(text), Err(ConfigError::Email), "{text:?}");
    }
    let long = format!("{}@example.com", "a".repeat(250));
    assert_eq!(Email::try_from(long.as_str()), Err(ConfigError::Email));
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
fn an_email_with_a_non_ascii_character_is_refused_and_ascii_capitals_are_lowered() {
    for text in [
        "\u{212A}ate@example.com",
        "kate@\u{212A}.example.com",
        "j\u{00F6}rg@example.com",
    ] {
        assert_eq!(Email::try_from(text), Err(ConfigError::Email), "{text:?}");
    }
    assert_eq!(
        Email::try_from("KATE@Example.com").unwrap().as_str(),
        "kate@example.com"
    );
}

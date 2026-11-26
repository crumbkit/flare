use super::*;

#[test]
fn a_teams_object_key_is_a_name_under_teams() {
    let key = TeamsObjectKey::try_from("teams/directory.json").unwrap();
    assert_eq!(key.as_str(), "teams/directory.json");
    assert!(TeamsObjectKey::try_from("teams/a/b.json").is_ok());
}

#[test]
fn a_teams_object_key_outside_teams_or_with_nothing_after_it_is_refused() {
    for bad in [
        "",
        "teams/",
        "files/abc",
        "directory.json",
        "/teams/x",
        "teams",
    ] {
        assert_eq!(TeamsObjectKey::try_from(bad), Err(TeamsKeyError), "{bad:?}");
    }
}

#[test]
fn a_teams_object_key_past_1024_bytes_is_refused_and_one_at_it_is_admitted() {
    let at = format!("teams/{}", "a".repeat(1024 - "teams/".len()));
    assert!(TeamsObjectKey::try_from(at.as_str()).is_ok());
    let over = format!("{at}a");
    assert_eq!(TeamsObjectKey::try_from(over.as_str()), Err(TeamsKeyError));
}

#[test]
fn a_teams_object_key_with_a_control_character_is_refused() {
    assert_eq!(TeamsObjectKey::try_from("teams/a\nb"), Err(TeamsKeyError));
}

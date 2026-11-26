use super::{Key, entry_of, grantee_of};
use flare_core::{
    Digest, Person, Position, PrincipalId, Reader, ShareId, ShareVersion, Timestamp, TokenId,
    TokenLabel,
};
use proptest::prelude::*;
use std::collections::HashSet;

fn principal(text: &str) -> PrincipalId {
    PrincipalId::try_from(text).unwrap()
}

fn every_key(
    share: &ShareId,
    other: &ShareId,
    person: &PrincipalId,
    created: Timestamp,
    version: ShareVersion,
) -> Vec<Key> {
    vec![
        Key::share(share),
        Key::version(share, version),
        Key::grant(share, person),
        Key::grant_count(share),
        Key::share_count(person),
        Key::owner_entry(person, created, other),
        Key::grantee_entry(person, created, other),
        Key::code(&Digest::from_bytes([share.as_str().as_bytes()[0]; 32])),
        Key::token(&TokenId::try_from(share.as_str()).unwrap()),
        Key::label_slot(
            &Person::try_from("a@x.com").unwrap(),
            &TokenLabel::try_from(person.as_str()).unwrap(),
        ),
        Key::sequence(),
        Key::pair(share, other),
        Key::link(share, Position::new(created.as_secs())),
        Key::index_entry(
            &Reader::Person(person.clone()),
            share,
            Position::new(created.as_secs()),
        ),
        Key::index_marker(&Reader::Person(person.clone()), share, other),
    ]
}

fn printable() -> impl Strategy<Value = PrincipalId> {
    "[!-~]{1,40}".prop_map(|text| principal(&text))
}

proptest! {
    #[test]
    fn no_two_item_kinds_build_equal_keys(
        share in any::<[u8; 16]>(),
        other in any::<[u8; 16]>(),
        person in printable(),
        created in any::<u64>(),
        version in 1..=u32::MAX,
    ) {
        let keys = every_key(
            &ShareId::from_random(share),
            &ShareId::from_random(other),
            &person,
            Timestamp::from_secs(created),
            ShareVersion::try_from(version).unwrap(),
        );
        let distinct: HashSet<(String, String)> =
            keys.iter().map(|key| (key.pk.clone(), key.sk.clone())).collect();
        prop_assert_eq!(distinct.len(), keys.len());
    }

    #[test]
    fn an_entry_sort_key_reads_back_its_time_and_share(
        share in any::<[u8; 16]>(),
        created in any::<u64>(),
        person in printable(),
    ) {
        let id = ShareId::from_random(share);
        let at = Timestamp::from_secs(created);
        let key = Key::owner_entry(&person, at, &id);
        prop_assert_eq!(entry_of(&key.sk), Some((at, id)));
    }

    #[test]
    fn a_grant_sort_key_reads_back_its_grantee(person in printable()) {
        let key = Key::grant(&ShareId::from_random([7; 16]), &person);
        prop_assert_eq!(grantee_of(&key.sk), Some(person));
    }
}

#[test]
fn entry_keys_sort_newest_first_and_then_by_share() {
    let person = principal("a@x.com");
    let (low, high) = (ShareId::from_random([1; 16]), ShareId::from_random([2; 16]));
    let mut keys = [
        Key::grantee_entry(&person, Timestamp::from_secs(5), &high),
        Key::grantee_entry(&person, Timestamp::from_secs(9), &low),
        Key::grantee_entry(&person, Timestamp::from_secs(5), &low),
        Key::grantee_entry(&person, Timestamp::from_secs(u64::MAX), &low),
        Key::grantee_entry(&person, Timestamp::from_secs(0), &low),
    ];
    keys.sort_by(|a, b| a.sk.cmp(&b.sk));
    let order: Vec<_> = keys.iter().map(|key| entry_of(&key.sk).unwrap()).collect();
    assert_eq!(
        order,
        [
            (Timestamp::from_secs(u64::MAX), low.clone()),
            (Timestamp::from_secs(9), low.clone()),
            (Timestamp::from_secs(5), low.clone()),
            (Timestamp::from_secs(5), high),
            (Timestamp::from_secs(0), low),
        ]
    );
}

#[test]
fn the_keys_have_their_documented_layout() {
    let id = ShareId::from_random([0xab; 16]);
    let person = principal("a@x.com");
    let hex = "ab".repeat(16);
    assert_eq!(Key::share(&id).pk, format!("SHARE#{hex}"));
    assert_eq!(Key::share(&id).sk, "META");
    let second = ShareVersion::try_from(2).unwrap();
    assert_eq!(Key::version(&id, second).sk, "V#0000000002");
    assert_eq!(Key::grant(&id, &person).sk, "G#a@x.com");
    assert_eq!(Key::grant_count(&id).sk, "N#grants");
    assert_eq!(Key::share_count(&person).pk, "OWN#a@x.com");
    assert_eq!(Key::share_count(&person).sk, "N#shares");
    let entry = Key::grantee_entry(&person, Timestamp::from_secs(0), &id);
    assert_eq!(entry.pk, "TO#a@x.com");
    assert_eq!(entry.sk, format!("E#{}#{hex}", u64::MAX));
}

#[test]
fn grants_of_an_address_and_a_longer_address_have_different_keys() {
    let id = ShareId::from_random([3; 16]);
    let short = Key::grant(&id, &principal("a@x.com"));
    let long = Key::grant(&id, &principal("a@x.com.au"));
    assert_ne!(short, long);
    assert_eq!(grantee_of("N#grants"), None);
    assert_eq!(entry_of("N#shares"), None);
    assert_eq!(entry_of("E#12#short"), None);
}

#[test]
fn link_keys_sort_in_position_order() {
    let flare = ShareId::from_random([3; 16]);
    let mut keys: Vec<String> = [u64::MAX, 10, 9, 0, 100]
        .iter()
        .map(|at| Key::link(&flare, Position::new(*at)).sk)
        .collect();
    keys.sort();
    let positions: Vec<String> = [0, 9, 10, 100, u64::MAX]
        .iter()
        .map(|at| Key::link(&flare, Position::new(*at)).sk)
        .collect();
    assert_eq!(keys, positions);
}

#[test]
fn a_group_grant_keys_its_entry_in_the_team_partition_or_the_everyone_partition() {
    let share = ShareId::from_random([7; 16]);
    let team = flare_core::Group::Team(flare_core::TeamName::try_from("design").unwrap());
    let created = Timestamp::from_secs(5);
    let team_entry = Key::grantee_entry(&team.principal_id(), created, &share);
    assert_eq!(team_entry.pk, "TEAM#design");
    assert_eq!(Key::grant(&share, &team.principal_id()).sk, "G#team:design");
    let everyone = flare_core::Group::Everyone.principal_id();
    assert_eq!(
        Key::grantee_entry(&everyone, created, &share).pk,
        "EVERYONE"
    );
    assert_eq!(Key::grant(&share, &everyone).sk, "G#everyone");
    let person = principal("ana@example.com");
    assert_eq!(
        Key::grantee_entry(&person, created, &share).pk,
        "TO#ana@example.com"
    );
}

#[test]
fn a_teams_items_and_a_members_pair_have_their_own_keys() {
    let team = flare_core::TeamName::try_from("design").unwrap();
    let ana = Person::try_from("ana@example.com").unwrap();
    assert_eq!(
        Key::team(&team),
        Key {
            pk: "TEAM#design".to_owned(),
            sk: "META".to_owned()
        }
    );
    let (in_team, in_person) = Key::member_pair(&team, &ana);
    assert_eq!(in_team.pk, "TEAM#design");
    assert_eq!(in_team.sk, "M#ana@example.com");
    assert_eq!(in_person.pk, "MEMBER#ana@example.com");
    assert_eq!(in_person.sk, "T#design");
    assert_eq!(super::member_of("M#ana@example.com"), Some(ana));
    assert_eq!(super::team_of("T#design"), Some(team));
}

#[test]
fn a_username_members_pair_and_label_slot_key_on_the_username() {
    let team = flare_core::TeamName::try_from("design").unwrap();
    let ana = Person::try_from("ana").unwrap();
    let (in_team, in_person) = Key::member_pair(&team, &ana);
    assert_eq!(in_team.sk, "M#ana");
    assert_eq!(in_person.pk, "MEMBER#ana");
    assert_eq!(super::member_of("M#ana"), Some(ana.clone()));
    let label = TokenLabel::try_from("laptop").unwrap();
    let slot = Key::label_slot(&ana, &label);
    assert!(slot.pk.ends_with("ana"), "{slot:?}");
}

#[test]
fn index_keys_have_their_documented_layout_and_sort_in_position_order_per_flare() {
    let reader = Reader::Person(principal("a@x.com"));
    let flare = ShareId::from_random([0xab; 16]);
    let response = ShareId::from_random([0xcd; 16]);
    let (flare_hex, response_hex) = ("ab".repeat(16), "cd".repeat(16));
    let entry = Key::index_entry(&reader, &flare, Position::new(7));
    assert_eq!(entry.pk, "ACT#a@x.com");
    assert_eq!(entry.sk, format!("A#{flare_hex}#{:020}", 7));
    let marker = Key::index_marker(&reader, &response, &flare);
    assert_eq!(marker.pk, "ACT#a@x.com");
    assert_eq!(marker.sk, format!("R#{response_hex}#{flare_hex}"));
    let mut keys: Vec<String> = [u64::MAX, 10, 9, 0]
        .iter()
        .map(|at| Key::index_entry(&reader, &flare, Position::new(*at)).sk)
        .collect();
    keys.sort();
    let positions: Vec<String> = [0, 9, 10, u64::MAX]
        .iter()
        .map(|at| Key::index_entry(&reader, &flare, Position::new(*at)).sk)
        .collect();
    assert_eq!(keys, positions);
}

#[test]
fn a_group_readers_index_has_its_own_partition_apart_from_every_person() {
    let flare = ShareId::from_random([0xab; 16]);
    let team = Reader::Group(flare_core::Group::Team(
        flare_core::TeamName::try_from("design").unwrap(),
    ));
    let everyone = Reader::Group(flare_core::Group::Everyone);
    assert_eq!(
        Key::index_entry(&team, &flare, Position::new(1)).pk,
        "ACT#team:design"
    );
    assert_eq!(
        Key::index_entry(&everyone, &flare, Position::new(1)).pk,
        "ACT#everyone"
    );
}

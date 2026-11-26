//! A deployment that names people by username: shares, grants, teams,
//! everyone grants, responses, creators and `from:` all key on the
//! username, and a principal with no username is refused.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemorySequence,
    MemoryShareStore, MemoryTeamStore,
};
use flare_core::{
    Admission, ByIdentity, CreatorDomain, CreatorList, DocumentText, Email, FlareError, FlareTeams,
    Flares, Group, IdentityKind, MailDomain, NewFlare, Person, Principal, PrincipalId,
    RecipientError, Recipients, Role, SearchError, ShareContent, ShareError, ShareId, ShareTitle,
    Standing, TeamName, Teams, Timestamp,
};
use flare_query::parse;

type Memory = Flares<
    MemoryShareStore,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
    FlareTeams<MemoryTeamStore>,
    CreatorList,
>;

fn person(text: &str) -> Person {
    Person::try_from(text).unwrap()
}

/// A username principal whose sign-in verified no email.
fn user(name: &str) -> Principal {
    Principal::with_person(PrincipalId::try_from(name).unwrap(), person(name), None)
}

/// A username principal whose sign-in verified `email`.
fn user_at(name: &str, email: &str) -> Principal {
    Principal::with_person(
        PrincipalId::try_from(name).unwrap(),
        person(name),
        Some(Email::try_from(email).unwrap()),
    )
}

fn id(byte: u8) -> ShareId {
    ShareId::from_random([byte; 16])
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

fn flare(byte: u8, with: &[&str], groups: Vec<Group>, re: Vec<ShareId>) -> NewFlare {
    NewFlare::new(
        id(byte),
        ShareTitle::try_from(format!("Flare {byte}").as_str()).unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        with.iter().map(|text| person(text)).collect(),
        re,
    )
    .with_groups(groups)
}

/// A username deployment whose creators are ana, bo and everyone whose
/// verified email is at example.com, under `admission`.
fn usernames(admission: Admission) -> Memory {
    let creators = CreatorList::listed(
        vec![person("ana"), person("bo")],
        vec![CreatorDomain::try_from("@example.com").unwrap()],
    );
    let sequence = MemorySequence::new();
    Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::with_sequence(sequence.clone()),
        MemoryCommentStore::with_sequence(sequence),
        MemoryCursorStore::new(),
        creators,
    )
    .with_teams(FlareTeams::new(MemoryTeamStore::new()))
    .admitting(admission, false)
    .with_identity(IdentityKind::Username)
}

fn domain() -> Admission {
    Admission::Domain(MailDomain::try_from("example.com").unwrap())
}

fn opens(flares: &Memory, who: &Principal, byte: u8) -> Result<Standing, ShareError> {
    block_on(flares.shares().open(who, &id(byte), None)).map(|opened| opened.standing)
}

#[test]
fn a_username_shares_with_another_username_who_opens_it() {
    let flares = usernames(Admission::Listed);
    let shared =
        block_on(flares.share(&user("ana"), flare(1, &["bo"], vec![], vec![]), at(1))).unwrap();
    assert_eq!(shared.share.owner().as_str(), "ana");
    assert_eq!(shared.granted, vec![person("bo")]);
    assert_eq!(opens(&flares, &user("bo"), 1), Ok(Standing::Reader));
    assert_eq!(opens(&flares, &user("cy"), 1), Err(ShareError::NotFound));
}

#[test]
fn a_username_owner_grants_admin_and_revokes_a_username() {
    let flares = usernames(Admission::Listed);
    block_on(flares.share(&user("ana"), flare(1, &[], vec![], vec![]), at(1))).unwrap();
    let shares = flares.shares();
    block_on(shares.grant(&user("ana"), &id(1), &person("bo"), Role::Admin, at(2))).unwrap();
    assert_eq!(opens(&flares, &user("bo"), 1), Ok(Standing::Admin));
    let revoked = block_on(flares.revoke(&user("ana"), &id(1), &[person("bo")], &[])).unwrap();
    assert_eq!(revoked.people, vec![person("bo")]);
    assert_eq!(opens(&flares, &user("bo"), 1), Err(ShareError::NotFound));
}

#[test]
fn a_username_creates_a_team_adds_a_member_and_the_member_opens_a_team_share() {
    let flares = usernames(Admission::Listed);
    let design = TeamName::try_from("design").unwrap();
    let created = block_on(flares.create_team(&user("ana"), design.clone(), at(1))).unwrap();
    assert_eq!(created.owner(), &person("ana"));
    let teams = flares.shares().teams();
    assert_eq!(
        block_on(teams.add(&person("ana"), &design, &person("bo"))),
        Ok(true)
    );
    let group = Group::Team(design);
    block_on(flares.share(&user("ana"), flare(1, &[], vec![group], vec![]), at(2))).unwrap();
    assert_eq!(opens(&flares, &user("bo"), 1), Ok(Standing::Reader));
    assert_eq!(opens(&flares, &user("cy"), 1), Err(ShareError::NotFound));
}

#[test]
fn a_domain_audience_opens_only_to_a_username_whose_verified_email_is_at_the_domain() {
    let flares = usernames(domain());
    block_on(flares.share(
        &user("ana"),
        flare(1, &[], vec![Group::Everyone], vec![]),
        at(1),
    ))
    .unwrap();
    assert_eq!(
        opens(&flares, &user_at("cy", "cy@example.com"), 1),
        Ok(Standing::Reader)
    );
    assert_eq!(
        opens(&flares, &user_at("dee", "dee@outside.example"), 1),
        Err(ShareError::NotFound)
    );
    assert_eq!(opens(&flares, &user("eli"), 1), Err(ShareError::NotFound));
}

#[test]
fn a_response_is_granted_to_the_username_owner_of_the_flare_it_responds_to() {
    let flares = usernames(Admission::Listed);
    block_on(flares.share(&user("ana"), flare(1, &["bo"], vec![], vec![]), at(1))).unwrap();
    block_on(flares.share(&user("bo"), flare(2, &[], vec![], vec![id(1)]), at(2))).unwrap();
    assert_eq!(opens(&flares, &user("ana"), 2), Ok(Standing::Reader));
}

#[test]
fn a_username_or_a_verified_email_at_a_listed_domain_may_create_and_others_may_not() {
    let flares = usernames(Admission::Listed);
    assert!(block_on(flares.share(&user("ana"), flare(1, &[], vec![], vec![]), at(1))).is_ok());
    let at_domain = user_at("cy", "cy@example.com");
    assert!(block_on(flares.share(&at_domain, flare(2, &[], vec![], vec![]), at(2))).is_ok());
    let outside = user_at("dee", "dee@outside.example");
    assert_eq!(
        block_on(flares.share(&outside, flare(3, &[], vec![], vec![]), at(3))).err(),
        Some(FlareError::NotACreator)
    );
}

#[test]
fn from_me_and_from_a_username_find_shares_by_their_username_owner() {
    let flares = usernames(Admission::Listed);
    block_on(flares.share(&user("ana"), flare(1, &["bo"], vec![], vec![]), at(1))).unwrap();
    block_on(flares.share(&user("bo"), flare(2, &[], vec![], vec![]), at(2))).unwrap();
    let recipients = ByIdentity(IdentityKind::Username);
    let search = |text: &str| {
        block_on(flares.search(&user("bo"), &parse(text).unwrap(), &recipients, false, 50))
    };
    let mine = search("from:me").unwrap();
    assert_eq!(mine.hits.len(), 1);
    assert_eq!(mine.hits[0].share.id(), &id(2));
    let anas = search("from:ana").unwrap();
    assert_eq!(anas.hits.len(), 1);
    assert_eq!(anas.hits[0].share.id(), &id(1));
    assert!(matches!(
        search("from:ana@example.org"),
        Err(SearchError::Invalid(_))
    ));
}

#[test]
fn by_identity_resolves_only_its_own_kind() {
    let usernames = ByIdentity(IdentityKind::Username);
    assert_eq!(block_on(usernames.resolve("Ana")), Ok(person("ana")));
    assert_eq!(
        block_on(usernames.resolve("ana@example.org")),
        Err(RecipientError::NotAUsername)
    );
    let emails = ByIdentity(IdentityKind::Email);
    assert_eq!(
        block_on(emails.resolve("ana@example.org")),
        Ok(person("ana@example.org"))
    );
    assert_eq!(
        block_on(emails.resolve("ana")),
        Err(RecipientError::NotAnAddress)
    );
}

#[test]
fn an_email_principal_under_the_username_kind_has_no_person() {
    let flares = usernames(Admission::Listed);
    let email = Email::try_from("ana@example.org").unwrap();
    let mail = Principal::with_email(email.principal_id(), email);
    assert_eq!(
        block_on(flares.share(&mail, flare(1, &[], vec![], vec![]), at(1))).err(),
        Some(FlareError::Share(ShareError::NoPerson))
    );
    block_on(flares.share(&user("ana"), flare(2, &[], vec![], vec![]), at(2))).unwrap();
    assert_eq!(opens(&flares, &mail, 2), Err(ShareError::NoPerson));
}

#[test]
fn a_username_principal_under_the_email_kind_has_no_person() {
    let flares = usernames(Admission::Listed).with_identity(IdentityKind::Email);
    assert_eq!(
        block_on(flares.share(&user("ana"), flare(1, &[], vec![], vec![]), at(1))).err(),
        Some(FlareError::Share(ShareError::NoPerson))
    );
}

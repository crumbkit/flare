//! Teams and group grants over the in-memory stores: who manages a team,
//! who opens a share granted to a team or to everyone, and what removing a
//! member or deleting a team ends.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemorySequence,
    MemoryShareStore, MemoryTeamStore,
};
use flare_core::{
    Admission, AllAdmitted, CreatorList, DocumentText, Email, FlareError, FlareTeams, Flares,
    Group, MAX_MEMBERS, MAX_TEAMS_JOINED, MAX_TEAMS_OWNED, MailDomain, NewFlare, Position,
    Principal, PrincipalId, Role, ShareContent, ShareError, ShareId, ShareTitle, Standing,
    TeamName, Teams, TeamsError, Timestamp,
};

type Memory = Flares<
    MemoryShareStore,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
    FlareTeams<MemoryTeamStore>,
    CreatorList,
>;

fn email(text: &str) -> Email {
    Email::try_from(text).unwrap()
}

fn person(address: &str) -> Principal {
    Principal::with_email(PrincipalId::try_from(address).unwrap(), email(address))
}

fn alice() -> Principal {
    person("alice@example.com")
}

fn bob() -> Principal {
    person("bob@example.com")
}

fn carol() -> Principal {
    person("carol@example.com")
}

fn outsider() -> Principal {
    person("ola@outside.example")
}

fn team(name: &str) -> TeamName {
    TeamName::try_from(name).unwrap()
}

fn design() -> Group {
    Group::Team(team("design"))
}

fn id(byte: u8) -> ShareId {
    ShareId::from_random([byte; 16])
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

fn flare(byte: u8, with: &[&str], groups: Vec<Group>) -> NewFlare {
    NewFlare::new(
        id(byte),
        ShareTitle::try_from(format!("Flare {byte}").as_str()).unwrap(),
        ShareContent::Document(DocumentText::try_from("# Notes\n").unwrap()),
        with.iter().map(|text| email(text)).collect(),
        Vec::new(),
    )
    .with_groups(groups)
}

/// A service whose creators are alice and bob, under `admission`.
fn memory_admitting(admission: Admission) -> Memory {
    let creators = CreatorList::only(vec![email("alice@example.com"), email("bob@example.com")]);
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
    .admitting(admission)
}

fn domain(text: &str) -> MailDomain {
    MailDomain::try_from(text).unwrap()
}

fn memory() -> Memory {
    memory_admitting(Admission::Domain(domain("example.com")))
}

/// A service in which alice owns the team design, with bob as a member.
fn with_design() -> Memory {
    let flares = memory();
    block_on(flares.create_team(&alice(), team("design"), at(1))).unwrap();
    let teams = flares.shares().teams();
    block_on(teams.add(
        &email("alice@example.com"),
        &team("design"),
        &email("bob@example.com"),
    ))
    .unwrap();
    flares
}

fn opens(flares: &Memory, who: &Principal, byte: u8) -> Result<Standing, ShareError> {
    block_on(flares.shares().open(who, &id(byte), None)).map(|opened| opened.standing)
}

#[test]
fn a_creator_creates_a_team_owns_it_and_is_its_first_member() {
    let flares = memory();
    let created = block_on(flares.create_team(&alice(), team("design"), at(1))).unwrap();
    assert_eq!(created.owner(), &email("alice@example.com"));
    assert_eq!(created.members(), 1);
    let teams = flares.shares().teams();
    let (_, members) = block_on(teams.show(&email("alice@example.com"), &team("design"))).unwrap();
    assert_eq!(members, vec![email("alice@example.com")]);
}

#[test]
fn only_a_creator_may_create_a_team() {
    let flares = memory();
    let refused = block_on(flares.create_team(&carol(), team("design"), at(1)));
    assert_eq!(refused, Err(TeamsError::NotACreator));
}

#[test]
fn a_team_name_in_use_is_taken() {
    let flares = with_design();
    let taken = block_on(flares.create_team(&bob(), team("design"), at(2)));
    assert_eq!(taken, Err(TeamsError::Taken));
}

#[test]
fn a_repeated_create_by_the_teams_owner_answers_the_team_whole_and_keeps_its_members() {
    let flares = with_design();
    let again = block_on(flares.create_team(&alice(), team("design"), at(5))).unwrap();
    assert_eq!(again.owner(), &email("alice@example.com"));
    assert_eq!(again.members(), 2);
    assert_eq!(again.created(), at(1));
}

#[test]
fn only_the_owner_adds_or_removes_members_and_a_stranger_sees_no_team() {
    let flares = with_design();
    let teams = flares.shares().teams();
    let by_member = block_on(teams.add(
        &email("bob@example.com"),
        &team("design"),
        &email("carol@example.com"),
    ));
    assert_eq!(by_member, Err(TeamsError::Forbidden));
    let by_stranger = block_on(teams.add(
        &email("carol@example.com"),
        &team("design"),
        &email("carol@example.com"),
    ));
    assert_eq!(by_stranger, Err(TeamsError::NotFound));
    let missing = block_on(teams.show(&email("carol@example.com"), &team("nobody")));
    let hidden = block_on(teams.show(&email("carol@example.com"), &team("design")));
    assert_eq!(missing.map(|_| ()), Err(TeamsError::NotFound));
    assert_eq!(hidden.map(|_| ()), Err(TeamsError::NotFound));
}

#[test]
fn a_member_may_leave_and_the_owner_may_not() {
    let flares = with_design();
    let teams = flares.shares().teams();
    let bob_address = email("bob@example.com");
    assert_eq!(
        block_on(teams.remove(&bob_address, &team("design"), &bob_address)),
        Ok(true)
    );
    let alice_address = email("alice@example.com");
    assert_eq!(
        block_on(teams.remove(&alice_address, &team("design"), &alice_address)),
        Err(TeamsError::OwnerCannotLeave)
    );
}

#[test]
fn adding_a_present_member_again_writes_nothing() {
    let flares = with_design();
    let teams = flares.shares().teams();
    let again = block_on(teams.add(
        &email("alice@example.com"),
        &team("design"),
        &email("bob@example.com"),
    ));
    assert_eq!(again, Ok(false));
    let (shown, _) = block_on(teams.show(&email("alice@example.com"), &team("design"))).unwrap();
    assert_eq!(shown.members(), 2);
}

#[test]
fn a_team_holds_at_most_its_member_limit() {
    let flares = memory();
    block_on(flares.create_team(&alice(), team("big"), at(1))).unwrap();
    let teams = flares.shares().teams();
    let owner = email("alice@example.com");
    for number in 1..MAX_MEMBERS {
        let member = email(&format!("p{number}@example.com"));
        block_on(teams.add(&owner, &team("big"), &member)).unwrap();
    }
    let full = block_on(teams.add(&owner, &team("big"), &email("late@example.com")));
    assert_eq!(full, Err(TeamsError::TooManyMembers));
}

#[test]
fn a_person_owns_at_most_the_owned_limit_of_teams() {
    let flares = memory();
    for number in 0..MAX_TEAMS_OWNED {
        let name = team(&format!("t{number}"));
        block_on(flares.create_team(&alice(), name, at(1))).unwrap();
    }
    let refused = block_on(flares.create_team(&alice(), team("one-more"), at(2)));
    assert_eq!(refused, Err(TeamsError::TooManyOwned));
    assert_eq!(
        block_on(flares.shares().teams().exists(&team("one-more"))),
        Ok(false)
    );
}

#[test]
fn a_person_belongs_to_at_most_the_joined_limit_of_teams() {
    let flares = memory();
    let carol_address = email("carol@example.com");
    let teams = flares.shares().teams();
    for number in 0..MAX_TEAMS_JOINED {
        let name = team(&format!("j{number}"));
        let owner = if number % 2 == 0 { alice() } else { bob() };
        block_on(flares.create_team(&owner, name.clone(), at(1))).unwrap();
        let by = owner.email().unwrap().clone();
        block_on(teams.add(&by, &name, &carol_address)).unwrap();
    }
    block_on(flares.create_team(&alice(), team("last"), at(2))).unwrap();
    let refused = block_on(teams.add(&email("alice@example.com"), &team("last"), &carol_address));
    assert_eq!(refused, Err(TeamsError::TooManyJoined));
}

#[test]
fn a_member_opens_a_share_granted_to_their_team_and_a_stranger_does_not() {
    let flares = with_design();
    block_on(flares.share(&alice(), flare(1, &[], vec![design()]), at(2))).unwrap();
    assert_eq!(opens(&flares, &bob(), 1), Ok(Standing::Reader));
    assert_eq!(opens(&flares, &carol(), 1), Err(ShareError::NotFound));
}

#[test]
fn a_removed_member_is_refused_on_the_next_request() {
    let flares = with_design();
    block_on(flares.share(&alice(), flare(1, &[], vec![design()]), at(2))).unwrap();
    assert_eq!(opens(&flares, &bob(), 1), Ok(Standing::Reader));
    let teams = flares.shares().teams();
    block_on(teams.remove(
        &email("alice@example.com"),
        &team("design"),
        &email("bob@example.com"),
    ))
    .unwrap();
    assert_eq!(opens(&flares, &bob(), 1), Err(ShareError::NotFound));
}

#[test]
fn a_direct_grant_keeps_access_after_its_holder_leaves_the_team() {
    let flares = with_design();
    block_on(flares.share(&alice(), flare(1, &[], vec![design()]), at(2))).unwrap();
    block_on(flares.share(
        &alice(),
        flare(2, &["bob@example.com"], vec![design()]),
        at(3),
    ))
    .unwrap();
    let bob_address = email("bob@example.com");
    let teams = flares.shares().teams();
    block_on(teams.remove(&bob_address, &team("design"), &bob_address)).unwrap();
    assert_eq!(opens(&flares, &bob(), 1), Err(ShareError::NotFound));
    assert_eq!(opens(&flares, &bob(), 2), Ok(Standing::Reader));
}

#[test]
fn a_person_outside_a_team_cannot_name_it_and_a_missing_team_answers_the_same() {
    let flares = with_design();
    block_on(flares.create_team(&bob(), team("ops"), at(2))).unwrap();
    let outside = block_on(flares.share(
        &alice(),
        flare(1, &[], vec![Group::Team(team("ops"))]),
        at(3),
    ));
    let missing = block_on(flares.share(
        &alice(),
        flare(1, &[], vec![Group::Team(team("nobody"))]),
        at(3),
    ));
    assert_eq!(
        outside.err(),
        Some(FlareError::Share(ShareError::TeamNotFound))
    );
    assert_eq!(
        missing.err(),
        Some(FlareError::Share(ShareError::TeamNotFound))
    );
    assert_eq!(
        block_on(flares.shares().open(&alice(), &id(1), None)).map(|_| ()),
        Err(ShareError::NotFound),
        "nothing is created when a named team is refused"
    );
}

#[test]
fn an_everyone_grant_opens_to_its_audience_and_refuses_anyone_outside_it() {
    let flares = memory();
    block_on(flares.share(&alice(), flare(1, &[], vec![Group::Everyone]), at(1))).unwrap();
    assert_eq!(opens(&flares, &carol(), 1), Ok(Standing::Reader));
    assert_eq!(opens(&flares, &outsider(), 1), Err(ShareError::NotFound));
}

#[test]
fn an_everyone_grant_keeps_its_audience_after_the_admit_rule_is_widened() {
    let stores = memory();
    block_on(stores.share(&alice(), flare(1, &[], vec![Group::Everyone]), at(1))).unwrap();
    let widened = stores.admitting(Admission::Anyone);
    assert_eq!(opens(&widened, &carol(), 1), Ok(Standing::Reader));
    assert_eq!(opens(&widened, &outsider(), 1), Err(ShareError::NotFound));
    let refused =
        block_on(widened.grant_all(&alice(), &id(1), &[], &[Group::Everyone], &[], at(2)));
    assert_eq!(
        refused.err(),
        None,
        "the held everyone grant is skipped, so nothing is refused"
    );
    let new_share = block_on(widened.share(&alice(), flare(2, &[], vec![Group::Everyone]), at(3)));
    assert_eq!(
        new_share.err(),
        Some(FlareError::Share(ShareError::EveryoneUnbounded))
    );
}

#[test]
fn an_admitted_audience_opens_only_while_the_rule_is_a_list() {
    let listed = memory_admitting(Admission::Listed);
    block_on(listed.share(&alice(), flare(1, &[], vec![Group::Everyone]), at(1))).unwrap();
    assert_eq!(opens(&listed, &outsider(), 1), Ok(Standing::Reader));
    let widened = listed.admitting(Admission::Anyone);
    assert_eq!(opens(&widened, &outsider(), 1), Err(ShareError::NotFound));
}

#[test]
fn under_an_anyone_rule_an_everyone_grant_is_refused_before_any_write() {
    let flares = memory_admitting(Admission::Anyone);
    let refused = block_on(flares.share(&alice(), flare(1, &[], vec![Group::Everyone]), at(1)));
    assert_eq!(
        refused.err(),
        Some(FlareError::Share(ShareError::EveryoneUnbounded))
    );
    assert_eq!(
        block_on(flares.shares().open(&alice(), &id(1), None)).map(|_| ()),
        Err(ShareError::NotFound)
    );
}

#[test]
fn an_admin_cannot_add_or_remove_an_everyone_grant() {
    let flares = with_design();
    block_on(flares.share(&alice(), flare(1, &[], vec![Group::Everyone]), at(1))).unwrap();
    block_on(flares.shares().grant(
        &alice(),
        &id(1),
        &email("bob@example.com"),
        Role::Admin,
        at(2),
    ))
    .unwrap();
    let removed = block_on(flares.revoke(&bob(), &id(1), &[], &[Group::Everyone]));
    assert_eq!(
        removed.err(),
        Some(FlareError::Share(ShareError::Forbidden))
    );
    block_on(flares.share(&alice(), flare(2, &[], Vec::new()), at(3))).unwrap();
    block_on(flares.shares().grant(
        &alice(),
        &id(2),
        &email("bob@example.com"),
        Role::Admin,
        at(4),
    ))
    .unwrap();
    let added = block_on(flares.grant_all(&bob(), &id(2), &[], &[Group::Everyone], &[], at(5)));
    assert_eq!(added.err(), Some(FlareError::Share(ShareError::Forbidden)));
}

#[test]
fn an_admin_may_grant_a_team_they_belong_to() {
    let flares = with_design();
    block_on(flares.share(&alice(), flare(1, &[], Vec::new()), at(1))).unwrap();
    block_on(flares.shares().grant(
        &alice(),
        &id(1),
        &email("bob@example.com"),
        Role::Admin,
        at(2),
    ))
    .unwrap();
    let granted = block_on(flares.grant_all(&bob(), &id(1), &[], &[design()], &[], at(3))).unwrap();
    assert_eq!(granted.groups, vec![design()]);
}

#[test]
fn a_group_grant_is_a_reader_grant_that_counts_toward_the_share_limit() {
    let flares = with_design();
    block_on(flares.share(
        &alice(),
        flare(1, &[], vec![design(), Group::Everyone]),
        at(1),
    ))
    .unwrap();
    let listed = block_on(flares.shares().listed_grants(&alice(), &id(1))).unwrap();
    assert_eq!(listed.len(), 2);
    assert!(
        listed
            .iter()
            .all(|entry| entry.grant.role() == Role::Reader)
    );
    assert!(listed.iter().all(|entry| !entry.dormant));
}

#[test]
fn shares_with_me_lists_team_shares_and_leaves_out_everyone_shares_unless_asked() {
    let flares = with_design();
    block_on(flares.share(&alice(), flare(1, &[], vec![design()]), at(1))).unwrap();
    block_on(flares.share(&alice(), flare(2, &[], vec![Group::Everyone]), at(2))).unwrap();
    let plain = block_on(flares.shares().shared_with_me(&bob(), false)).unwrap();
    let ids: Vec<ShareId> = plain
        .items()
        .iter()
        .map(|entry| entry.share.id().clone())
        .collect();
    assert_eq!(ids, vec![id(1)]);
    let all = block_on(flares.shares().shared_with_me(&bob(), true)).unwrap();
    let ids: Vec<ShareId> = all
        .items()
        .iter()
        .map(|entry| entry.share.id().clone())
        .collect();
    assert_eq!(ids, vec![id(2), id(1)]);
    let own = block_on(flares.shares().shared_with_me(&alice(), true)).unwrap();
    assert!(
        own.items().is_empty(),
        "an owner's own shares are not shared with them"
    );
}

#[test]
fn a_team_member_sees_the_responses_to_a_flare_granted_to_the_team() {
    let flares = with_design();
    block_on(flares.share(&alice(), flare(1, &[], vec![design()]), at(1))).unwrap();
    let response = NewFlare::new(
        id(2),
        ShareTitle::try_from("Reply").unwrap(),
        ShareContent::Document(DocumentText::try_from("Yes\n").unwrap()),
        Vec::new(),
        vec![id(1)],
    )
    .with_groups(vec![design()]);
    block_on(flares.share(&alice(), response, at(2))).unwrap();
    let page = block_on(flares.activity(&bob(), &[id(1)], Position::new(0), 50)).unwrap();
    assert_eq!(page.items.len(), 1);
    let teams = flares.shares().teams();
    let bob_address = email("bob@example.com");
    block_on(teams.remove(&bob_address, &team("design"), &bob_address)).unwrap();
    let gone = block_on(flares.activity(&bob(), &[id(1)], Position::new(0), 50));
    assert_eq!(gone.err(), Some(FlareError::NotFound));
}

#[test]
fn revoke_removes_a_persons_grant_and_a_repeat_finishes_without_error() {
    let flares = with_design();
    block_on(flares.share(
        &alice(),
        flare(1, &["carol@example.com"], Vec::new()),
        at(1),
    ))
    .unwrap();
    let carol_address = email("carol@example.com");
    let first =
        block_on(flares.revoke(&alice(), &id(1), std::slice::from_ref(&carol_address), &[]))
            .unwrap();
    assert_eq!(first.people, vec![carol_address.clone()]);
    assert_eq!(opens(&flares, &carol(), 1), Err(ShareError::NotFound));
    let again = block_on(flares.revoke(&alice(), &id(1), &[carol_address], &[])).unwrap();
    assert!(again.people.is_empty());
}

#[test]
fn the_default_teams_answer_managed_elsewhere_for_every_change() {
    let sequence = MemorySequence::new();
    let flares = Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::with_sequence(sequence.clone()),
        MemoryCommentStore::with_sequence(sequence),
        MemoryCursorStore::new(),
        AllAdmitted,
    );
    let refused = block_on(flares.create_team(&alice(), team("design"), at(1)));
    assert_eq!(refused, Err(TeamsError::ManagedElsewhere));
}

#[test]
fn a_share_admin_outside_a_team_sees_its_grant_hidden_and_a_member_admin_sees_the_name() {
    let flares = with_design();
    block_on(flares.share(
        &alice(),
        flare(1, &["carol@example.com"], vec![design()]),
        at(1),
    ))
    .unwrap();
    block_on(flares.shares().grant(
        &alice(),
        &id(1),
        &email("carol@example.com"),
        Role::Admin,
        at(2),
    ))
    .unwrap();
    let hidden = |who: &Principal| -> Vec<bool> {
        block_on(flares.shares().listed_grants(who, &id(1)))
            .unwrap()
            .iter()
            .filter(|entry| Group::of_grantee(entry.grant.grantee()).is_some())
            .map(|entry| entry.hidden)
            .collect()
    };
    assert_eq!(hidden(&carol()), vec![true]);
    assert_eq!(hidden(&alice()), vec![false]);
    let teams = flares.shares().teams();
    block_on(teams.add(
        &email("alice@example.com"),
        &team("design"),
        &email("carol@example.com"),
    ))
    .unwrap();
    assert_eq!(hidden(&carol()), vec![false]);
}

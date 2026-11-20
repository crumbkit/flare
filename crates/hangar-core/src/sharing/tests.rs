//! Unit tests of the decision tables.

use super::{
    Authority, Decision, Refusal, ShareAction, Standing, Step, Target, TargetAction, decide_actor,
    decide_target,
};
use crate::grant::Role;

const STANDINGS: [Standing; 4] = [
    Standing::Owner,
    Standing::Admin,
    Standing::Reader,
    Standing::Stranger,
];

const ACTIONS: [ShareAction; 6] = [
    ShareAction::Read,
    ShareAction::ListGrants,
    ShareAction::Publish,
    ShareAction::Grant,
    ShareAction::Revoke,
    ShareAction::Leave,
];

/// What a request decides after both steps, given the target it would read.
fn decide(
    standing: Standing,
    action: ShareAction,
    target: Option<(TargetAction, Target)>,
) -> Decision {
    match decide_actor(standing, action) {
        Step::Allow => Decision::Allow,
        Step::Refuse(refusal) => Decision::Refuse(refusal),
        Step::ReadTarget(authority) => match target {
            Some((on_target, target)) => decide_target(authority, on_target, target),
            None => Decision::Refuse(Refusal::Forbidden),
        },
    }
}

const ALLOW: Decision = Decision::Allow;
const NOT_FOUND: Decision = Decision::Refuse(Refusal::NotFound);
const FORBIDDEN: Decision = Decision::Refuse(Refusal::Forbidden);
const OWNER: Decision = Decision::Refuse(Refusal::OwnerCannotBeChanged);
const NO_GRANT: Decision = Decision::Refuse(Refusal::GrantNotFound);

#[test]
fn every_standing_and_action_without_a_target_gives_the_decision_in_the_table() {
    use ShareAction::{Leave, ListGrants, Publish, Read};
    let table = [
        (Standing::Owner, Read, ALLOW),
        (Standing::Stranger, Read, NOT_FOUND),
        (Standing::Admin, Read, ALLOW),
        (Standing::Reader, Read, ALLOW),
        (Standing::Owner, ListGrants, ALLOW),
        (Standing::Admin, ListGrants, ALLOW),
        (Standing::Reader, ListGrants, FORBIDDEN),
        (Standing::Owner, ListGrants, ALLOW),
        (Standing::Stranger, ListGrants, NOT_FOUND),
        (Standing::Owner, Publish, ALLOW),
        (Standing::Admin, Publish, FORBIDDEN),
        (Standing::Owner, Publish, ALLOW),
        (Standing::Reader, Publish, FORBIDDEN),
        (Standing::Owner, Publish, ALLOW),
        (Standing::Stranger, Publish, NOT_FOUND),
        (Standing::Admin, Leave, ALLOW),
        (Standing::Reader, Leave, ALLOW),
        (Standing::Owner, Leave, OWNER),
        (Standing::Admin, Leave, ALLOW),
        (Standing::Stranger, Leave, NOT_FOUND),
    ];
    for (standing, action, expected) in table {
        assert_eq!(
            decide(standing, action, None),
            expected,
            "{standing:?} {action:?}"
        );
    }
}

#[test]
fn every_standing_and_action_on_a_target_gives_the_decision_in_the_table() {
    use Role::{Admin, Reader};
    use ShareAction::{Grant, Revoke};
    use TargetAction as On;
    let none = Target::Person(None);
    let reader = Target::Person(Some(Reader));
    let admin = Target::Person(Some(Admin));
    let table = [
        // Grant Reader to a person with no grant or a Reader grant.
        (Standing::Owner, Grant, On::Grant(Reader), none, ALLOW),
        (Standing::Admin, Grant, On::Grant(Reader), none, ALLOW),
        (Standing::Reader, Grant, On::Grant(Reader), none, FORBIDDEN),
        (Standing::Admin, Grant, On::Grant(Reader), reader, ALLOW),
        (
            Standing::Stranger,
            Grant,
            On::Grant(Reader),
            none,
            NOT_FOUND,
        ),
        // Grant Admin, or change an Admin to Reader.
        (Standing::Admin, Grant, On::Grant(Admin), none, FORBIDDEN),
        (Standing::Owner, Grant, On::Grant(Admin), none, ALLOW),
        (Standing::Admin, Grant, On::Grant(Admin), reader, FORBIDDEN),
        (Standing::Owner, Grant, On::Grant(Admin), reader, ALLOW),
        (Standing::Admin, Grant, On::Grant(Reader), admin, FORBIDDEN),
        (Standing::Owner, Grant, On::Grant(Reader), admin, ALLOW),
        // Revoke a Reader grant.
        (Standing::Reader, Revoke, On::Revoke, reader, FORBIDDEN),
        (Standing::Admin, Revoke, On::Revoke, reader, ALLOW),
        (Standing::Stranger, Revoke, On::Revoke, reader, NOT_FOUND),
        (Standing::Owner, Revoke, On::Revoke, reader, ALLOW),
        // Revoke an Admin grant.
        (Standing::Admin, Revoke, On::Revoke, admin, FORBIDDEN),
        (Standing::Owner, Revoke, On::Revoke, admin, ALLOW),
        // Revoke a person with no grant.
        (Standing::Owner, Revoke, On::Revoke, none, NO_GRANT),
        (Standing::Owner, Revoke, On::Revoke, reader, ALLOW),
        // The owner as the target.
        (
            Standing::Owner,
            Grant,
            On::Grant(Reader),
            Target::Owner,
            OWNER,
        ),
        (Standing::Owner, Grant, On::Grant(Reader), none, ALLOW),
        (Standing::Admin, Revoke, On::Revoke, Target::Owner, OWNER),
        (Standing::Admin, Revoke, On::Revoke, reader, ALLOW),
        (
            Standing::Reader,
            Revoke,
            On::Revoke,
            Target::Owner,
            FORBIDDEN,
        ),
        (
            Standing::Stranger,
            Revoke,
            On::Revoke,
            Target::Owner,
            NOT_FOUND,
        ),
    ];
    for (standing, action, on, target, expected) in table {
        assert_eq!(
            decide(standing, action, Some((on, target))),
            expected,
            "{standing:?} {on:?} {target:?}"
        );
    }
}

#[test]
fn decide_actor_answers_a_reader_forbidden_for_every_action_except_read_and_leave() {
    for action in ACTIONS {
        let step = decide_actor(Standing::Reader, action);
        if matches!(action, ShareAction::Read | ShareAction::Leave) {
            assert_eq!(step, Step::Allow, "{action:?}");
        } else {
            assert_eq!(step, Step::Refuse(Refusal::Forbidden), "{action:?}");
        }
    }
}

#[test]
fn decide_actor_answers_a_stranger_not_found_for_every_action() {
    for action in ACTIONS {
        assert_eq!(
            decide_actor(Standing::Stranger, action),
            Step::Refuse(Refusal::NotFound),
            "{action:?}"
        );
    }
}

#[test]
fn an_admin_may_grant_a_reader_and_not_an_admin_and_the_owner_gets_the_opposite_answer_only_for_admin()
 {
    let none = Target::Person(None);
    let reader = TargetAction::Grant(Role::Reader);
    let admin = TargetAction::Grant(Role::Admin);
    assert_eq!(decide_target(Authority::Admin, reader, none), ALLOW);
    assert_eq!(decide_target(Authority::Admin, admin, none), FORBIDDEN);
    assert_eq!(decide_target(Authority::Owner, reader, none), ALLOW);
    assert_eq!(decide_target(Authority::Owner, admin, none), ALLOW);
}

#[test]
fn the_owner_cannot_be_revoked_by_anyone_and_no_one_can_grant_the_owner() {
    for standing in STANDINGS {
        let refusal = match standing {
            Standing::Stranger => NOT_FOUND,
            Standing::Reader => FORBIDDEN,
            Standing::Owner | Standing::Admin => OWNER,
        };
        for on in [
            TargetAction::Revoke,
            TargetAction::Grant(Role::Reader),
            TargetAction::Grant(Role::Admin),
        ] {
            let action = match on {
                TargetAction::Revoke => ShareAction::Revoke,
                TargetAction::Grant(_) => ShareAction::Grant,
            };
            assert_eq!(
                decide(standing, action, Some((on, Target::Owner))),
                refusal,
                "{standing:?} {on:?}"
            );
        }
    }
}

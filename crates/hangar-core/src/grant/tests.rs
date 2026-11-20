//! Unit tests of the grant types.

use super::{Grant, GrantRevoke, GrantWrite, Granter, Revoker, Role};
use crate::name::PrincipalId;
use crate::share::ShareId;
use crate::time::Timestamp;

fn person(text: &str) -> PrincipalId {
    PrincipalId::try_from(text).unwrap()
}

#[test]
fn the_debug_output_of_a_grant_and_its_writes_holds_no_address() {
    let share = ShareId::from_random([2; 16]);
    let at = Timestamp::from_secs(1_700_000_000);
    let grant = Grant::new(
        share.clone(),
        person("bob@example.com"),
        Role::Reader,
        person("alice@example.com"),
        at,
    );
    let write = GrantWrite::new(
        share.clone(),
        person("bob@example.com"),
        Role::Admin,
        Some(Role::Reader),
        Granter::Admin(person("alice@example.com")),
        at,
    );
    let revoke = GrantRevoke::new(
        share,
        person("bob@example.com"),
        Role::Reader,
        Revoker::Owner(person("alice@example.com")),
    );
    for debug in [
        format!("{grant:?}"),
        format!("{write:?}"),
        format!("{revoke:?}"),
        format!("{:?}", Granter::Owner(person("alice@example.com"))),
        format!("{:?}", Revoker::Admin(person("alice@example.com"))),
    ] {
        assert!(!debug.contains("example.com"), "{debug}");
        assert!(debug.contains("<principal>"), "{debug}");
    }
}

#[test]
fn a_granter_and_a_revoker_name_the_person_they_act_for() {
    let alice = person("alice@example.com");
    assert_eq!(Granter::Owner(alice.clone()).principal(), &alice);
    assert_eq!(Granter::Admin(alice.clone()).principal(), &alice);
    assert_eq!(Revoker::Owner(alice.clone()).admin(), None);
    assert_eq!(Revoker::Admin(alice.clone()).admin(), Some(&alice));
    assert_eq!(Revoker::Grantee.admin(), None);
    assert_eq!(Granter::Admin(alice.clone()).admin(), Some(&alice));
    assert_eq!(Granter::Owner(alice).admin(), None);
}

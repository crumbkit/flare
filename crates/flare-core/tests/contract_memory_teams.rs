//! Runs the team store suite against the in-memory team store, so the
//! suite is known to pass on a correct implementation.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::contract::team_store::{self, TeamFixture};
use flare_core::memory::MemoryTeamStore;

macro_rules! suite {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            let fixture = TeamFixture::standard().unwrap();
            block_on(team_store::$name(&MemoryTeamStore::new(), &fixture)).unwrap();
        }
    )*};
}

suite!(
    a_created_team_reads_back_with_its_owner_as_its_only_member,
    a_name_is_taken_while_its_team_is_active_or_deleting,
    member_pairs_always_agree_after_adds_and_removes,
    adding_a_present_member_writes_nothing,
    removing_an_absent_member_writes_nothing,
    a_full_team_refuses_another_member_and_writes_nothing,
    a_deleted_or_missing_team_refuses_a_member,
    a_deleting_team_keeps_its_owner_and_its_state,
    forgetting_a_deleting_team_with_no_members_frees_its_name,
    a_transfer_needs_the_owner_and_a_member,
);

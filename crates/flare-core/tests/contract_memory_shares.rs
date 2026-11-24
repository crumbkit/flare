//! Runs the share, grant and file store suites against the in-memory
//! stores, so each suite is known to pass on a correct implementation.
#![cfg(test)]

use flare_core::contract::file_store::{self, FileFixture};
use flare_core::contract::grant_store::{self, GrantFixture};
use flare_core::contract::share_store::{self, ShareFixture};
use flare_core::contract::{Threads, block_on};
use flare_core::memory::{MemoryFileStore, MemoryGrantStore, MemoryShareStore};

macro_rules! suite {
    ($module:ident, $store:ident, $fixture:ident; $($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            let fixture = $fixture::standard().unwrap();
            block_on($module::$name(&$store::new(), &fixture)).unwrap();
        }
    )*};
}

macro_rules! concurrent_suite {
    ($module:ident, $store:ident, $fixture:ident; $($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            let fixture = $fixture::standard().unwrap();
            block_on($module::$name(&$store::new(), &fixture, &Threads)).unwrap();
        }
    )*};
}

mod shares {
    use super::*;

    suite!(share_store, MemoryShareStore, ShareFixture;
        a_created_share_reads_back,
        the_first_version_holds_the_created_content,
        a_taken_share_id_changes_nothing,
        publish_gives_the_next_version,
        a_publish_leaves_older_versions_unchanged,
        every_version_reads_back,
        publishing_to_an_unknown_share_creates_nothing,
        an_unknown_share_or_version_is_not_found,
        the_owners_list_holds_only_their_shares_newest_first,
        a_share_is_listed_once_created,
        a_share_always_has_its_first_version,
    );

    concurrent_suite!(share_store, MemoryShareStore, ShareFixture;
        concurrent_creates_of_one_id_give_one_id_taken,
        concurrent_publishes_get_consecutive_versions,
        the_share_limit_per_owner_holds,
        the_version_limit_holds,
    );
}

mod grants {
    use super::*;

    suite!(grant_store, MemoryGrantStore, GrantFixture;
        a_grant_reads_back,
        a_grant_from_the_current_role_replaces_it,
        a_grant_from_a_stale_role_is_changed,
        a_revoke_is_visible_to_every_read,
        a_revoke_from_a_stale_role_is_changed,
        a_grant_under_a_stale_admin_is_refused,
        a_grant_under_a_current_admin_is_applied,
        a_revoke_under_a_stale_admin_is_refused,
        the_grant_limit_holds,
        a_share_list_holds_only_its_grants,
        a_group_grant_keeps_its_audience,
        a_persons_list_holds_their_grants_newest_first,
        a_role_change_keeps_the_first_grant_time,
        a_revoke_and_a_regrant_leave_no_stale_entry,
        a_grant_on_an_unseen_share_is_applied,
        a_leave_applies_under_the_expected_role_alone,
        stale_authority_comes_before_changed,
        a_list_shows_its_truncation_and_order,
        a_persons_grants_page_in_order_past_the_listing_limit,
    );

    concurrent_suite!(grant_store, MemoryGrantStore, GrantFixture;
        concurrent_grants_from_no_grant_give_one_changed,
        a_grant_and_a_revoke_together_leave_a_consistent_state,
        an_admins_grant_never_lands_after_their_revoke,
        concurrent_grants_at_the_limit_give_one_too_many,
    );
}

mod files {
    use super::*;

    suite!(file_store, MemoryFileStore, FileFixture;
        a_stored_file_reads_back,
        a_file_is_not_found_under_another_share,
        a_taken_file_id_changes_nothing,
        an_unknown_file_is_not_found,
        the_largest_file_reads_back,
    );

    concurrent_suite!(file_store, MemoryFileStore, FileFixture;
        concurrent_puts_of_one_id_give_one_id_taken,
    );
}

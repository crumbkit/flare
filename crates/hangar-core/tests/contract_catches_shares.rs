//! Runs the share, grant and file store suites against stores that each
//! break one clause, and asserts the clause that fails.
#![cfg(test)]

#[path = "support/flawed_grants.rs"]
mod flawed_grants;
#[path = "support/flawed_shares.rs"]
mod flawed_shares;

use flawed_grants::{FlawedGrants, GrantFlaw};
use flawed_shares::{FileFlaw, FlawedFiles, FlawedShares, ShareFlaw};
use hangar_core::contract::file_store::{self, FileFixture};
use hangar_core::contract::grant_store::{self, GrantFixture};
use hangar_core::contract::share_store::{self, ShareFixture};
use hangar_core::contract::{Clause, Threads, Violation, block_on};

fn share_clause(
    flaw: ShareFlaw,
    run: impl AsyncFnOnce(&FlawedShares, &ShareFixture) -> Result<(), Violation>,
) -> Clause {
    let fixture = ShareFixture::standard().unwrap();
    let store = FlawedShares::new(flaw);
    block_on(run(&store, &fixture)).unwrap_err().clause()
}

fn grant_clause(
    flaw: GrantFlaw,
    run: impl AsyncFnOnce(&FlawedGrants, &GrantFixture) -> Result<(), Violation>,
) -> Clause {
    let fixture = GrantFixture::standard().unwrap();
    let store = FlawedGrants::new(flaw);
    block_on(run(&store, &fixture)).unwrap_err().clause()
}

fn file_clause(
    flaw: FileFlaw,
    run: impl AsyncFnOnce(&FlawedFiles, &FileFixture) -> Result<(), Violation>,
) -> Clause {
    let fixture = FileFixture::standard().unwrap();
    let store = FlawedFiles::new(flaw);
    block_on(run(&store, &fixture)).unwrap_err().clause()
}

#[test]
fn a_share_store_that_reuses_a_version_number_in_a_race_fails_the_concurrent_publish_clause() {
    let clause = share_clause(ShareFlaw::ReusesANumberInARace, async |store, fixture| {
        share_store::concurrent_publishes_get_consecutive_versions(store, fixture, &Threads).await
    });
    assert_eq!(clause, Clause::ConcurrentPublishesGetConsecutiveVersions);
}

#[test]
fn a_share_store_that_overwrites_a_taken_id_fails_the_taken_id_clause() {
    let clause = share_clause(ShareFlaw::OverwritesATakenId, async |store, fixture| {
        share_store::a_taken_share_id_changes_nothing(store, fixture).await
    });
    assert_eq!(clause, Clause::ATakenShareIdChangesNothing);
}

#[test]
fn a_share_store_that_changes_the_first_version_on_publish_fails_the_older_versions_clause() {
    let clause = share_clause(
        ShareFlaw::ChangesTheFirstVersionOnPublish,
        async |store, fixture| {
            share_store::a_publish_leaves_older_versions_unchanged(store, fixture).await
        },
    );
    assert_eq!(clause, Clause::APublishLeavesOlderVersionsUnchanged);
}

#[test]
fn a_share_store_that_lists_other_owners_fails_the_owners_list_clause() {
    let clause = share_clause(ShareFlaw::ListsOtherOwners, async |store, fixture| {
        share_store::the_owners_list_holds_only_their_shares_newest_first(store, fixture).await
    });
    assert_eq!(clause, Clause::TheOwnersListHoldsOnlyTheirSharesNewestFirst);
}

#[test]
fn a_share_store_that_lists_a_share_late_fails_the_listed_once_created_clause() {
    let clause = share_clause(ShareFlaw::ListsLate, async |store, fixture| {
        share_store::a_share_is_listed_once_created(store, fixture).await
    });
    assert_eq!(clause, Clause::AShareIsListedOnceCreated);
}

#[test]
fn a_share_store_that_publishes_without_moving_latest_fails_the_next_version_clause() {
    let clause = share_clause(ShareFlaw::KeepsLatest, async |store, fixture| {
        share_store::publish_gives_the_next_version(store, fixture).await
    });
    assert_eq!(clause, Clause::PublishGivesTheNextVersion);
}

#[test]
fn a_share_store_that_writes_no_first_version_fails_the_first_version_clause() {
    let clause = share_clause(ShareFlaw::WritesNoFirstVersion, async |store, fixture| {
        share_store::a_share_always_has_its_first_version(store, fixture).await
    });
    assert_eq!(clause, Clause::AShareAlwaysHasItsFirstVersion);
}

#[test]
fn a_share_store_that_serves_a_version_above_latest_fails_the_not_found_clause() {
    let clause = share_clause(ShareFlaw::ServesAboveLatest, async |store, fixture| {
        share_store::an_unknown_share_or_version_is_not_found(store, fixture).await
    });
    assert_eq!(clause, Clause::AnUnknownShareOrVersionIsNotFound);
}

#[test]
fn a_share_store_without_a_share_limit_fails_the_share_limit_clause() {
    let clause = share_clause(ShareFlaw::NoShareLimit, async |store, fixture| {
        share_store::the_share_limit_per_owner_holds(store, fixture, &Threads).await
    });
    assert_eq!(clause, Clause::TheShareLimitPerOwnerHolds);
}

#[test]
fn a_share_store_without_a_version_limit_fails_the_version_limit_clause() {
    let clause = share_clause(ShareFlaw::NoVersionLimit, async |store, fixture| {
        share_store::the_version_limit_holds(store, fixture, &Threads).await
    });
    assert_eq!(clause, Clause::TheVersionLimitHolds);
}

/// One test per grant store flaw, each naming the clause that catches it.
macro_rules! grant_catches {
    ($($test:ident: $flaw:ident => $clause:ident by $function:ident $(($runner:expr))?;)*) => {$(
        #[test]
        fn $test() {
            let clause = grant_clause(GrantFlaw::$flaw, async |store, fixture| {
                grant_store::$function(store, fixture $(, $runner)?).await
            });
            assert_eq!(clause, Clause::$clause);
        }
    )*};
}

grant_catches! {
    a_grant_store_that_ignores_the_expected_role_on_grant_fails_the_stale_role_clause:
        IgnoresExpectedOnGrant => AGrantFromAStaleRoleIsChanged by a_grant_from_a_stale_role_is_changed;
    a_grant_store_that_ignores_the_expected_role_on_revoke_fails_the_stale_revoke_clause:
        IgnoresExpectedOnRevoke => ARevokeFromAStaleRoleIsChanged by a_revoke_from_a_stale_role_is_changed;
    a_grant_store_that_ignores_the_admin_on_grant_fails_the_stale_admin_grant_clause:
        IgnoresAdminOnGrant => AGrantUnderAStaleAdminIsRefused by a_grant_under_a_stale_admin_is_refused;
    a_grant_store_that_ignores_the_admin_on_revoke_fails_the_stale_admin_revoke_clause:
        IgnoresAdminOnRevoke => ARevokeUnderAStaleAdminIsRefused by a_revoke_under_a_stale_admin_is_refused;
    a_grant_store_without_a_limit_fails_the_limit_clause:
        NoLimit => TheGrantLimitHolds by the_grant_limit_holds;
    a_grant_store_that_keeps_the_count_after_a_revoke_fails_the_limit_clause:
        KeepsCountAfterRevoke => TheGrantLimitHolds by the_grant_limit_holds;
    a_grant_store_that_refuses_a_role_change_at_the_limit_fails_the_limit_clause:
        RefusesRoleChangeAtLimit => TheGrantLimitHolds by the_grant_limit_holds;
    a_grant_store_that_answers_changed_before_stale_authority_fails_the_order_clause:
        ChangedBeforeStaleAuthority => StaleAuthorityComesBeforeChanged by stale_authority_comes_before_changed;
    a_grant_store_that_keeps_a_person_entry_after_a_revoke_fails_the_revoke_clause:
        KeepsPersonEntryAfterRevoke => ARevokeIsVisibleToEveryRead by a_revoke_is_visible_to_every_read;
    a_grant_store_that_shows_a_grant_on_another_share_fails_the_share_list_clause:
        ShowsGrantOnOtherShares => AShareListHoldsOnlyItsGrants by a_share_list_holds_only_its_grants;
    a_grant_store_that_loses_a_grant_in_a_race_fails_the_concurrent_grant_clause:
        LosesAGrantInARace => ConcurrentGrantsFromNoGrantGiveOneChanged by concurrent_grants_from_no_grant_give_one_changed (&Threads);
    a_grant_store_that_rewrites_the_first_grant_time_fails_the_role_change_clause:
        RewritesCreated => ARoleChangeKeepsTheFirstGrantTime by a_role_change_keeps_the_first_grant_time;
    a_grant_store_that_revives_a_revoked_grant_fails_the_regrant_clause:
        RevivesARevokedGrant => ARevokeAndARegrantLeaveNoStaleEntry by a_revoke_and_a_regrant_leave_no_stale_entry;
    a_grant_store_that_refuses_an_unseen_share_fails_the_unseen_share_clause:
        RefusesAnUnseenShare => AGrantOnAnUnseenShareIsApplied by a_grant_on_an_unseen_share_is_applied;
    a_grant_store_that_cuts_a_list_before_counting_fails_the_truncation_clause:
        WrongTruncation => AListShowsItsTruncationAndOrder by a_list_shows_its_truncation_and_order;
    a_grant_store_that_lists_the_oldest_first_fails_the_truncation_and_order_clause:
        WrongOrder => AListShowsItsTruncationAndOrder by a_list_shows_its_truncation_and_order;
    a_grant_store_that_caches_an_admin_fails_the_grant_after_an_admin_revoke_clause:
        CachesAdminAuthority => AnAdminsGrantNeverLandsAfterTheirRevoke by an_admins_grant_never_lands_after_their_revoke (&Threads);
}

#[test]
fn a_grant_store_that_caches_an_admin_passes_the_single_stale_admin_clause() {
    // The admitted twin: this clause revokes the admin before any grant
    // under them, so the cache never holds the admin and the store passes.
    // The clause an_admins_grant_never_lands_after_their_revoke grants under
    // the admin first, which fills the cache, and its sequential grant after
    // the revoke then lands; that grant is what tells the two stores apart.
    let fixture = GrantFixture::standard().unwrap();
    let store = FlawedGrants::new(GrantFlaw::CachesAdminAuthority);
    block_on(grant_store::a_grant_under_a_stale_admin_is_refused(
        &store, &fixture,
    ))
    .unwrap();
}

#[test]
fn a_file_store_that_ignores_the_share_fails_the_other_share_clause() {
    let clause = file_clause(FileFlaw::IgnoresTheShare, async |store, fixture| {
        file_store::a_file_is_not_found_under_another_share(store, fixture).await
    });
    assert_eq!(clause, Clause::AFileIsNotFoundUnderAnotherShare);
}

#[test]
fn a_file_store_that_overwrites_a_taken_id_fails_the_taken_id_clause() {
    let clause = file_clause(FileFlaw::OverwritesATakenId, async |store, fixture| {
        file_store::a_taken_file_id_changes_nothing(store, fixture).await
    });
    assert_eq!(clause, Clause::ATakenFileIdChangesNothing);
}

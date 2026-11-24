//! Runs every function of flare-core's share, grant, token, response and
//! team suites against `DynamoShareStore`, `DynamoGrantStore`,
//! `DynamoTokenStore`, `DynamoResponseStore` and `DynamoTeamStore` on the DynamoDB stand-in, as
//! flare's AGENTS.md asks of each new store, and checks what only a
//! durable table shows: a second store on the same table sees every write,
//! and a login code carries the expiry that the table's TTL reads.
#![cfg(test)]

#[path = "support/stand_in.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the stand-in support is test support: it panics on a broken fixture"
)]
pub mod stand_in;

use flare_aws::Stores;
use flare_core::contract::grant_store::{self, GrantFixture};
use flare_core::contract::response_store::{self, ResponseFixture, index as response_index};
use flare_core::contract::share_store::{self, ShareFixture};
use flare_core::contract::team_store::{self, TeamFixture};
use flare_core::contract::token_store::{self, TokenFixture};
use flare_core::{
    CODE_LIFE, Digest, DocumentText, Email, GrantStore, LoginCode, NewLink, NewToken, Position,
    Principal, ResponseStore, Role, ShareContent, ShareId, ShareStore, ShareTitle, ShareVersion,
    Shares, Timestamp, TokenId, TokenLabel, TokenStore,
};
use stand_in::{Joined, dynamodb_client, endpoints, fresh, fresh_config};

macro_rules! suite {
    ($module:ident, $field:ident, $fixture:ident; $($name:ident),* $(,)?) => {$(
        #[tokio::test(flavor = "multi_thread")]
        async fn $name() {
            let fixture = $fixture::standard().unwrap();
            let stores = fresh().await;
            $module::$name(&stores.$field, &fixture).await.unwrap();
        }
    )*};
}

macro_rules! concurrent_suite {
    ($module:ident, $field:ident, $fixture:ident; $($name:ident),* $(,)?) => {$(
        #[tokio::test(flavor = "multi_thread")]
        async fn $name() {
            let fixture = $fixture::standard().unwrap();
            let stores = fresh().await;
            $module::$name(&stores.$field, &fixture, &Joined).await.unwrap();
        }
    )*};
}

mod shares {
    use super::*;

    suite!(share_store, shares, ShareFixture;
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

    concurrent_suite!(share_store, shares, ShareFixture;
        concurrent_creates_of_one_id_give_one_id_taken,
        concurrent_publishes_get_consecutive_versions,
        the_share_limit_per_owner_holds,
        the_version_limit_holds,
    );
}

mod teams {
    use super::*;

    suite!(team_store, teams, TeamFixture;
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
}

mod grants {
    use super::*;

    suite!(grant_store, grants, GrantFixture;
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

    concurrent_suite!(grant_store, grants, GrantFixture;
        concurrent_grants_from_no_grant_give_one_changed,
        a_grant_and_a_revoke_together_leave_a_consistent_state,
        an_admins_grant_never_lands_after_their_revoke,
        concurrent_grants_at_the_limit_give_one_too_many,
    );
}

mod tokens {
    use super::*;

    suite!(token_store, tokens, TokenFixture;
        a_code_is_used_once,
        an_expired_code_is_refused,
        an_unknown_code_is_refused,
        a_token_reads_back,
        an_unknown_token_is_not_found,
        a_revoke_keeps_its_first_time,
        a_revoke_by_another_owner_is_not_found,
        a_new_token_with_the_same_label_revokes_the_old,
    );

    concurrent_suite!(token_store, tokens, TokenFixture;
        concurrent_uses_of_one_code_give_one_success,
    );
}

mod responses {
    use super::*;

    suite!(response_store, responses, ResponseFixture;
        a_link_reads_back_under_its_flare,
        positions_increase_and_the_head_follows,
        a_repeated_link_keeps_its_first_position,
        a_flares_list_holds_only_newer_links_up_to_the_bound,
        a_flares_list_holds_only_its_links,
        a_responses_list_names_every_flare,
        the_limit_bounds_a_list_oldest_first,
    );

    concurrent_suite!(response_store, responses, ResponseFixture;
        concurrent_links_get_distinct_positions,
    );
}

mod index {
    use super::*;

    suite!(response_index, responses, ResponseFixture;
        an_entry_reads_back_only_for_its_reader_and_flare,
        each_entry_takes_a_new_position_and_the_head_follows,
        a_repeated_entry_keeps_its_first_position,
        unindex_removes_only_the_readers_entries_for_the_response,
        a_readers_list_holds_only_newer_entries_up_to_the_bound,
        an_index_of_many_entries_writes_each,
        pairs_name_the_links_from_each_response_to_the_flare,
    );

    concurrent_suite!(response_index, responses, ResponseFixture;
        concurrent_indexes_get_distinct_positions,
    );
}

fn person(address: &str) -> (Principal, Email) {
    let email = Email::try_from(address).unwrap();
    (
        Principal::with_email(email.principal_id(), email.clone()),
        email,
    )
}

fn document(text: &str) -> ShareContent {
    ShareContent::Document(DocumentText::try_from(text).unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_store_on_the_same_table_sees_every_write_of_the_first_including_a_revoke() {
    let config = fresh_config().await;
    let first = Stores::connect_at(&config, &endpoints());
    let second = Stores::connect_at(&config, &endpoints());
    let writer = Shares::new(first.shares, first.grants);
    let (owner, _) = person("owner@x.com");
    let (_, short) = person("a@x.com");
    let (_, long) = person("a@x.com.au");
    let id = ShareId::from_random([9; 16]);
    let title = ShareTitle::try_from("Plans").unwrap();
    let at = Timestamp::from_secs(100);
    writer
        .create(&owner, id.clone(), title, document("# One\n"), at)
        .await
        .unwrap();
    writer
        .grant(&owner, &id, &short, Role::Reader, at)
        .await
        .unwrap();
    writer
        .grant(&owner, &id, &long, Role::Admin, at)
        .await
        .unwrap();

    let share = second.shares.get(&id).await.unwrap();
    assert_eq!(share.owner(), &owner.id().clone());
    let short_grant = second.grants.get(&id, &short.principal_id()).await.unwrap();
    let long_grant = second.grants.get(&id, &long.principal_id()).await.unwrap();
    assert_eq!(short_grant.map(|grant| grant.role()), Some(Role::Reader));
    assert_eq!(long_grant.map(|grant| grant.role()), Some(Role::Admin));
    let listed = second
        .grants
        .granted_to(&short.principal_id())
        .await
        .unwrap();
    assert_eq!(listed.items().len(), 1);
    assert_eq!(listed.items()[0].role(), Role::Reader);

    writer.revoke(&owner, &id, &short).await.unwrap();
    assert_eq!(
        second.grants.get(&id, &short.principal_id()).await,
        Ok(None)
    );
    assert!(
        second
            .grants
            .granted_to(&short.principal_id())
            .await
            .unwrap()
            .items()
            .is_empty()
    );
    let remaining = second.grants.of_share(&id).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].grantee(), &long.principal_id());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_document_of_the_largest_size_in_multi_byte_characters_is_stored_and_read_back() {
    let stores = fresh().await;
    let shares = Shares::new(stores.shares, stores.grants);
    let (owner, _) = person("owner@x.com");
    // "é" is two bytes, so 131,072 of them are 262,144 bytes.
    let text = "é".repeat(131_072);
    assert_eq!(text.len(), 262_144);
    let id = ShareId::from_random([8; 16]);
    let title = ShareTitle::try_from("Large").unwrap();
    shares
        .create(
            &owner,
            id.clone(),
            title,
            document(&text),
            Timestamp::from_secs(1),
        )
        .await
        .unwrap();
    let second = shares
        .publish(&owner, &id, document(&text), Timestamp::from_secs(2))
        .await
        .unwrap();
    assert_eq!(second, ShareVersion::try_from(2).unwrap());
    let opened = shares.open(&owner, &id, Some(second)).await.unwrap();
    assert_eq!(opened.version.content(), &document(&text));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_store_on_the_same_table_sees_a_token_its_revoke_and_a_response_link() {
    let config = fresh_config().await;
    let first = Stores::connect_at(&config, &endpoints());
    let second = Stores::connect_at(&config, &endpoints());
    let (_, owner) = person("owner@x.com");
    let id = TokenId::from_random([3; 16]);
    let token = NewToken::new(
        id.clone(),
        Digest::from_bytes([4; 32]),
        owner.clone(),
        TokenLabel::try_from("laptop").unwrap(),
        Timestamp::from_secs(10),
    );
    first.tokens.create(token.clone()).await.unwrap();
    assert_eq!(second.tokens.get(&id).await, Ok(token.stored()));
    first
        .tokens
        .revoke(&id, &owner, Timestamp::from_secs(20))
        .await
        .unwrap();
    assert_eq!(
        second.tokens.get(&id).await.unwrap().revoked(),
        Some(Timestamp::from_secs(20))
    );

    let link = NewLink::new(
        ShareId::from_random([5; 16]),
        ShareId::from_random([6; 16]),
        ShareVersion::try_from(1).unwrap(),
        Timestamp::from_secs(30),
    );
    let stored = first.responses.link(link.clone()).await.unwrap();
    assert_eq!(second.responses.head().await, Ok(stored.position));
    let listed = second
        .responses
        .to_flare(&link.flare, Position::new(0), stored.position, 10)
        .await
        .unwrap();
    assert_eq!(listed, vec![stored]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_login_code_item_carries_its_expiry_in_the_tables_ttl_attribute() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    let (_, owner) = person("owner@x.com");
    let issued = Timestamp::from_secs(1_000);
    let code = LoginCode::new(Digest::from_bytes([7; 32]), owner, 4000, issued);
    stores.tokens.issue_code(code).await.unwrap();

    let scanned = dynamodb_client()
        .scan()
        .table_name(config.table().as_str())
        .send()
        .await
        .unwrap();
    let expiries: Vec<String> = scanned
        .items()
        .iter()
        .filter_map(|item| item.get(stand_in::TTL_ATTRIBUTE))
        .map(|value| value.as_n().unwrap().clone())
        .collect();
    assert_eq!(expiries, vec![(1_000 + CODE_LIFE).to_string()]);
}

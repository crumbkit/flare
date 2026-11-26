//! Runs the token and response store suites against the in-memory stores,
//! so each suite is known to pass on a correct implementation.
#![cfg(test)]

use flare_core::contract::response_store::{self, ResponseFixture, index as response_index};
use flare_core::contract::token_store::{self, TokenFixture};
use flare_core::contract::{People, Threads, block_on};
use flare_core::memory::{MemoryResponseStore, MemoryTokenStore};

macro_rules! suite {
    ($module:ident, $store:ident, $fixture:ident; $($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            let fixture = $fixture::standard().unwrap();
            block_on($module::$name(&$store::new(), &fixture)).unwrap();
        }
    )*};
}

macro_rules! people_suite {
    ($module:ident, $store:ident, $fixture:ident; $($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            for people in People::ALL {
                let fixture = $fixture::of(people).unwrap();
                block_on($module::$name(&$store::new(), &fixture)).unwrap();
            }
        }
    )*};
}

macro_rules! people_concurrent_suite {
    ($module:ident, $store:ident, $fixture:ident; $($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            for people in People::ALL {
                let fixture = $fixture::of(people).unwrap();
                block_on($module::$name(&$store::new(), &fixture, &Threads)).unwrap();
            }
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

mod tokens {
    use super::*;

    people_suite!(token_store, MemoryTokenStore, TokenFixture;
        a_code_is_used_once,
        an_expired_code_is_refused,
        an_unknown_code_is_refused,
        a_token_reads_back,
        an_unknown_token_is_not_found,
        a_revoke_keeps_its_first_time,
        a_revoke_by_another_owner_is_not_found,
        a_new_token_with_the_same_label_revokes_the_old,
    );

    people_concurrent_suite!(token_store, MemoryTokenStore, TokenFixture;
        concurrent_uses_of_one_code_give_one_success,
    );
}

mod responses {
    use super::*;

    suite!(response_store, MemoryResponseStore, ResponseFixture;
        a_link_reads_back_under_its_flare,
        positions_increase_and_the_head_follows,
        a_repeated_link_keeps_its_first_position,
        a_flares_list_holds_only_newer_links_up_to_the_bound,
        a_flares_list_holds_only_its_links,
        a_responses_list_names_every_flare,
        the_limit_bounds_a_list_oldest_first,
    );

    concurrent_suite!(response_store, MemoryResponseStore, ResponseFixture;
        concurrent_links_get_distinct_positions,
    );
}

mod index {
    use super::*;

    suite!(response_index, MemoryResponseStore, ResponseFixture;
        an_entry_reads_back_only_for_its_reader_and_flare,
        each_entry_takes_a_new_position_and_the_head_follows,
        a_repeated_entry_keeps_its_first_position,
        unindex_removes_only_the_readers_entries_for_the_response,
        a_readers_list_holds_only_newer_entries_up_to_the_bound,
        an_index_of_many_entries_writes_each,
        pairs_name_the_links_from_each_response_to_the_flare,
    );

    concurrent_suite!(response_index, MemoryResponseStore, ResponseFixture;
        concurrent_indexes_get_distinct_positions,
    );
}

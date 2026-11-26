//! Runs the session store and sealing key suites against the in-memory
//! stores, so each suite is known to pass on a correct implementation.
#![cfg(test)]

use flare_core::contract::sealing_keys::{self, KeyFixture};
use flare_core::contract::session_store::{self, SessionFixture};
use flare_core::contract::{People, Threads, block_on};
use flare_core::memory::{MemorySealingKeys, MemorySessionStore};

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

people_suite!(session_store, MemorySessionStore, SessionFixture;
    a_session_lives_twelve_hours_from_sign_in,
    a_request_slides_the_expiry_at_most_every_ten_minutes,
    a_session_ends_seven_days_after_sign_in,
    a_person_holds_eight_sessions,
    a_sign_in_replaces_the_old_session,
    removing_ends_sessions,
    holds_follows_live_sessions,
);

suite!(sealing_keys, MemorySealingKeys, KeyFixture;
    the_first_call_writes_key_one,
    rotation_waits_thirty_days,
    an_old_key_reads_for_one_hour_after_rotation,
);

#[test]
fn concurrent_calls_agree_on_one_key() {
    let fixture = KeyFixture::standard().unwrap();
    let store = MemorySealingKeys::new();
    block_on(sealing_keys::concurrent_calls_agree_on_one_key(
        &store, &fixture, &Threads,
    ))
    .unwrap();
}

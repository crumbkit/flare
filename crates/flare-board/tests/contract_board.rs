//! Runs every function of flare-core's record and page suites against a
//! `BoardStore` on a scratch board, as flare's AGENTS.md asks of each new
//! store.
#![cfg(test)]

#[path = "support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch board is test support: it panics on a broken fixture, and each test file uses part of it"
)]
pub mod scratch;

use flare_board::BoardStore;
use flare_core::contract::page_store::{self, PageFixture};
use flare_core::contract::record_store::{self, RecordFixture};
use flare_core::contract::{SeedRecord, Threads, block_on};
use scratch::ScratchBoard;

/// A record's body as the board stores it: pretty JSON with sorted keys and
/// a final newline.
fn stored(record: &serde_json::Value) -> String {
    let mut text = serde_json::to_string_pretty(record).unwrap();
    text.push('\n');
    text
}

fn task(title: &str) -> String {
    stored(&serde_json::json!({"$type": "tasks/task/v2", "title": title}))
}

fn record_fixture() -> RecordFixture {
    RecordFixture::with_bodies(&task("first"), &task("second"), &task("third")).unwrap()
}

fn page_fixture() -> PageFixture {
    PageFixture::with_bodies(
        &task("first"),
        &task("second"),
        &stored(&serde_json::json!({"$type": "notes/note/v1", "text": "a note"})),
    )
    .unwrap()
}

/// A scratch board that holds `seeds`, each written as the board stores it.
fn seeded(seeds: &[SeedRecord]) -> ScratchBoard {
    let board = ScratchBoard::new();
    for seed in seeds {
        assert!(seed.body().as_str().starts_with('{'));
        board.seed_record(seed.path().as_str(), seed.body().as_str());
    }
    board
}

macro_rules! record_suite {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            let fixture = record_fixture();
            let board = seeded(fixture.seeds());
            block_on(record_store::$name(&board.store(), &fixture)).unwrap();
        }
    )*};
}

record_suite!(
    read_returns_what_was_seeded,
    write_returns_a_new_blob_and_replaces_the_body,
    a_stale_blob_conflicts_and_changes_nothing,
    an_old_blob_conflicts_after_a_write,
    a_blob_never_repeats_for_one_record,
    a_rejected_write_changes_nothing,
    an_unknown_path_is_not_found,
    read_all_answers_each_path_as_read_does,
);

#[test]
fn concurrent_writes_from_one_blob_give_one_conflict() {
    let fixture = record_fixture();
    let board = seeded(fixture.seeds());
    block_on(
        record_store::concurrent_writes_from_one_blob_give_one_conflict(
            &board.store(),
            &fixture,
            &Threads,
        ),
    )
    .unwrap();
}

macro_rules! page_suite {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            let fixture = page_fixture();
            let board = seeded(fixture.seeds());
            let store: BoardStore = board.store();
            block_on(page_store::$name(&store, &store, &fixture)).unwrap();
        }
    )*};
}

page_suite!(
    publish_then_latest_returns_the_page,
    version_names_are_never_reused,
    move_binding_refuses_a_stale_pin,
    a_move_records_its_mover,
    republish_adds_a_version_newest_first,
    pages_lists_published_names,
    unknown_names_are_not_found,
    provenance_is_returned_unchanged,
    an_unresolved_origin_is_refused,
    a_binding_round_trips,
    a_taken_name_and_an_unknown_version_are_refused,
    a_misfit_is_refused_on_save,
    a_republish_leaves_a_pin_unchanged,
    move_binding_changes_only_the_pin,
    move_binding_refuses_a_misfit,
    bindings_of_lists_a_pages_bindings,
    a_two_fill_binding_is_checked_against_one_state,
);

macro_rules! vanished_suite {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            let fixture = page_fixture();
            let board = seeded(fixture.seeds());
            let empty = ScratchBoard::new();
            let store = board.store();
            block_on(page_store::$name(&store, &store, &empty.store(), &fixture)).unwrap();
        }
    )*};
}

vanished_suite!(
    check_fit_sees_a_vanished_record,
    move_binding_refuses_a_vanished_record,
);

macro_rules! concurrent_page_suite {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            let fixture = page_fixture();
            let board = seeded(fixture.seeds());
            let store = board.store();
            block_on(page_store::$name(&store, &store, &fixture, &Threads)).unwrap();
        }
    )*};
}

concurrent_page_suite!(
    concurrent_publishes_get_two_versions,
    concurrent_saves_of_one_name_give_one_name_taken,
    concurrent_moves_from_one_pin_give_one_moved,
);

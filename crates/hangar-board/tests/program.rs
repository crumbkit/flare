//! `BoardStore::check_program`: whether the board program can read history.
#![cfg(test)]

#[path = "support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch board is test support: it panics on a broken fixture, and each test file uses part of it"
)]
pub mod scratch;

use hangar_board::{Limits, ProgramError};
use hangar_core::contract::block_on;
use scratch::ScratchBoard;

/// What a board older than crumbkit/board e4ee3333 prints for `log --at`.
const USAGE_REFUSAL: &str = r#"printf '%s\n' '{"schema":"board/failure/v1","code":"board::cli::usage","message":"unexpected argument '"'"'--at'"'"' found","fields":{"argument":"--at"}}'
exit 2
"#;

#[test]
fn a_board_program_without_log_at_is_too_old() {
    let board = ScratchBoard::new();
    let stub = board.script("old-board", USAGE_REFUSAL);
    let store = board.store_with(&stub, Limits::default());
    assert_eq!(block_on(store.check_program()), Err(ProgramError::TooOld));
}

#[test]
fn the_test_board_program_reads_history() {
    let board = ScratchBoard::new();
    let store = board.store();
    assert_eq!(block_on(store.check_program()), Ok(()));
}

#[test]
fn a_board_program_that_prints_no_report_is_unavailable() {
    let board = ScratchBoard::new();
    let stub = board.script("broken-board", "echo not json\nexit 3\n");
    let store = board.store_with(&stub, Limits::default());
    assert_eq!(
        block_on(store.check_program()),
        Err(ProgramError::Unavailable)
    );
}

#[test]
fn a_board_program_that_refuses_for_another_reason_still_reads_history() {
    let board = ScratchBoard::new();
    let stub = board.script(
        "picky-board",
        r#"printf '%s\n' '{"schema":"board/failure/v1","code":"board::path::not_found","message":"no such path"}'
exit 1
"#,
    );
    let store = board.store_with(&stub, Limits::default());
    assert_eq!(block_on(store.check_program()), Ok(()));
}

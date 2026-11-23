//! The limits that `BoardStore` puts on its children, and how it opens.
#![cfg(test)]

#[path = "support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch board is test support: it panics on a broken fixture, and each test file uses part of it"
)]
pub mod scratch;

use flare_board::{BoardConfig, BoardStore, Committer, Limits, OpenError};
use flare_core::contract::block_on;
use flare_core::{
    BindingName, Principal, PrincipalId, ReadError, RecordBody, RecordPath, RecordStore,
    RecordType, WriteError, WriteProvenance,
};
use scratch::ScratchBoard;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn path(text: &str) -> RecordPath {
    RecordPath::try_from(text).unwrap()
}

fn seeded_task() -> ScratchBoard {
    let board = ScratchBoard::new();
    board.seed_record("/tasks/a", r#"{"$type":"tasks/task/v2","title":"first"}"#);
    board
}

fn limits(change: impl FnOnce(&mut Limits)) -> Limits {
    let mut limits = Limits::default();
    change(&mut limits);
    limits
}

fn write_title(store: &BoardStore, title: &str) -> Result<flare_core::Blob, WriteError> {
    let current = block_on(store.read(&path("/tasks/a"))).unwrap();
    let body = serde_json::json!({"$type": "tasks/task/v2", "title": title}).to_string();
    block_on(store.write(
        &path("/tasks/a"),
        &RecordBody::try_from(body.as_str()).unwrap(),
        &WriteProvenance::new(
            current.blob().clone(),
            Principal::new(PrincipalId::try_from("ada").unwrap()),
            BindingName::try_from("my-view").unwrap(),
        ),
    ))
}

#[test]
fn a_held_lock_answers_unavailable_within_the_lock_wait() {
    let board = seeded_task();
    let store = board.store_with(
        board.program(),
        limits(|limits| limits.lock_wait = Duration::from_secs(1)),
    );
    let lock = board.root().join(".git/index.lock");
    #[expect(
        clippy::disallowed_methods,
        reason = "the test holds git's index lock as another git process would"
    )]
    std::fs::write(&lock, "").unwrap();
    let started = Instant::now();
    let answer = write_title(&store, "second");
    let took = started.elapsed();
    std::fs::remove_file(&lock).unwrap();
    assert_eq!(answer, Err(WriteError::Unavailable));
    assert!(took < Duration::from_secs(6), "the write took {took:?}");
}

#[test]
fn a_free_lock_writes() {
    let board = seeded_task();
    let store = board.store_with(
        board.program(),
        limits(|limits| limits.lock_wait = Duration::from_secs(1)),
    );
    assert!(write_title(&store, "second").is_ok());
}

#[test]
fn a_child_past_its_deadline_is_stopped_and_answers_unavailable() {
    let board = seeded_task();
    let sleeper = board.script("sleeping-board", "sleep 30\n");
    let store = board.store_with(
        &sleeper,
        limits(|limits| limits.child_deadline = Duration::from_secs(1)),
    );
    let started = Instant::now();
    let answer = block_on(store.read(&path("/tasks/a")));
    let took = started.elapsed();
    assert_eq!(answer, Err(ReadError::Unavailable));
    assert!(took < Duration::from_secs(10), "the read took {took:?}");
}

#[test]
fn a_fifth_concurrent_call_waits_for_a_free_place() {
    let board = seeded_task();
    let marks = board.scratch().join("marks");
    std::fs::create_dir(&marks).unwrap();
    let real = board.program().display().to_string();
    let marks_text = marks.display().to_string();
    let counting = board.script(
        "counting-board",
        &format!(
            "mkdir \"{marks_text}/live.$$\"\n\
             ls -d \"{marks_text}\"/live.* | wc -l >> \"{marks_text}/counts\"\n\
             sleep 0.5\n\
             rmdir \"{marks_text}/live.$$\"\n\
             exec \"{real}\" \"$@\"\n"
        ),
    );
    let store = board.store_with(&counting, limits(|limits| limits.children = 4));
    let answers: Vec<_> = std::thread::scope(|scope| {
        let calls: Vec<_> = (0..5)
            .map(|_| scope.spawn(|| block_on(store.read_all(&[path("/tasks/missing")]))))
            .collect();
        calls.into_iter().map(|call| call.join().unwrap()).collect()
    });
    assert!(
        answers
            .iter()
            .all(|answer| answer == &[Err(ReadError::NotFound)])
    );
    let counts = std::fs::read_to_string(marks.join("counts")).unwrap();
    let most = counts
        .lines()
        .map(|line| line.trim().parse::<usize>().unwrap())
        .max()
        .unwrap();
    assert!(most <= 4, "{most} children ran at once: {counts}");
    assert_eq!(counts.lines().count(), 5);
}

#[test]
fn schema_returns_the_text_of_a_types_schema_file() {
    let board = ScratchBoard::new();
    let store = board.store();
    let text = block_on(store.schema(&RecordType::try_from("tasks/task/v2").unwrap())).unwrap();
    let read: serde_json::Value = serde_json::from_str(&text).unwrap();
    let seeded: serde_json::Value = serde_json::from_str(scratch::TASK_SCHEMA).unwrap();
    assert_eq!(read, seeded);
}

#[test]
fn schema_of_an_unknown_type_is_not_found() {
    let board = ScratchBoard::new();
    let store = board.store();
    let answer = block_on(store.schema(&RecordType::try_from("nobody/nothing/v1").unwrap()));
    assert_eq!(answer, Err(ReadError::NotFound));
}

fn config(root: PathBuf, program: PathBuf, board: &ScratchBoard) -> BoardConfig {
    BoardConfig::new(
        root,
        program,
        board.state(),
        scratch::search_path(),
        Committer::new("flare tests", "flare-tests@example.invalid").unwrap(),
    )
}

#[test]
fn open_refuses_a_directory_without_a_board_and_a_relative_program() {
    let board = ScratchBoard::new();
    let not_a_board = board.state();
    let refused = BoardStore::open(config(not_a_board, board.program().to_path_buf(), &board));
    assert!(
        matches!(refused, Err(OpenError::NotABoard(_))),
        "{refused:?}"
    );
    let relative = BoardStore::open(config(board.root(), PathBuf::from("board"), &board));
    assert!(
        matches!(relative, Err(OpenError::RelativeProgram(_))),
        "{relative:?}"
    );
}

#[test]
fn open_accepts_a_scratch_board() {
    let board = ScratchBoard::new();
    assert!(BoardStore::open(config(board.root(), board.program().to_path_buf(), &board)).is_ok());
}

#[test]
fn a_board_without_reads_at_a_commit_makes_read_all_unavailable() {
    let board = seeded_task();
    let real = board.program().display().to_string();
    let old = board.script(
        "old-board",
        &format!(
            r#"case "$*" in
  *--at*)
    printf '%s\n' '{{"schema":"board/failure/v1","code":"board::cli::usage","message":"unexpected argument --at","fields":{{}}}}'
    exit 2;;
esac
exec "{real}" "$@"
"#
        ),
    );
    let store = board.store_with(&old, Limits::default());
    assert_eq!(
        block_on(store.read_all(&[path("/tasks/a")])),
        [Err(ReadError::Unavailable)]
    );
}

/// Writes `text` as the board's `.board/board.json`.
fn configure(board: &ScratchBoard, text: &str) {
    #[expect(
        clippy::disallowed_methods,
        reason = "the test writes its board's configuration"
    )]
    std::fs::write(board.root().join(".board/board.json"), text).unwrap();
}

#[test]
fn open_refuses_a_board_whose_puts_are_not_committed_and_writes_nothing() {
    let board = seeded_task();
    configure(
        &board,
        r#"{"$type":"board/config/v1","commit_on_put":false}"#,
    );
    let before = board.git(&["status", "--porcelain", "--untracked-files=all"]);
    let refused = BoardStore::open(config(board.root(), board.program().to_path_buf(), &board));
    let after = board.git(&["status", "--porcelain", "--untracked-files=all"]);
    assert!(
        matches!(refused, Err(OpenError::CommitsOff(_))),
        "{refused:?}"
    );
    assert_eq!(before, after);
}

#[test]
fn open_accepts_a_board_that_says_it_commits_its_puts() {
    let board = seeded_task();
    configure(
        &board,
        r#"{"$type":"board/config/v1","commit_on_put":true}"#,
    );
    assert!(BoardStore::open(config(board.root(), board.program().to_path_buf(), &board)).is_ok());
}

#[test]
fn open_refuses_a_board_whose_configuration_does_not_parse() {
    let board = seeded_task();
    configure(&board, "{not json");
    let refused = BoardStore::open(config(board.root(), board.program().to_path_buf(), &board));
    assert!(
        matches!(refused, Err(OpenError::UnreadableConfig(_))),
        "{refused:?}"
    );
}

#[test]
fn a_board_program_without_log_at_is_refused_with_what_to_do() {
    let board = ScratchBoard::new();
    let old = board.script(
        "old-board",
        "printf '%s\\n' 'Usage: board log [OPTIONS] [PATH]' '  --limit <N>'\n",
    );
    let text = scratch::check_board(&old).unwrap_err();
    assert!(text.contains("log --at"), "{text}");
    assert!(text.contains("FLARE_TEST_BOARD"), "{text}");
    assert!(scratch::check_board(board.program()).is_ok());
}

use super::{Board, Rows};
use crate::Limits;
use crate::report::Failure;
use crate::wait::Gate;
use hangar_core::contract::block_on;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A board whose program is the shell script `body`, in a new directory
/// that the caller removes.
fn scripted(body: &str, limits: Limits) -> (Arc<Board>, PathBuf) {
    let directory = std::env::temp_dir().join(format!(
        "hb-unit-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let program = directory.join("board");
    #[expect(
        clippy::disallowed_methods,
        reason = "the test writes its fixture script"
    )]
    std::fs::write(&program, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let board = Board {
        root: directory.clone(),
        program,
        state: directory.clone(),
        search_path: "/usr/bin:/bin".into(),
        committer: ("hangar".into(), "hangar@example.invalid".into()),
        gate: Gate::new(limits.children),
        limits,
    };
    (Arc::new(board), directory)
}

#[test]
fn a_board_without_reads_at_a_commit_fails_with_the_usage_code() {
    let (board, directory) = scripted(
        "printf '%s' '{\"schema\":\"board/failure/v1\",\"code\":\"board::cli::usage\",\"message\":\"unexpected argument --at\"}'\nexit 2\n",
        Limits::default(),
    );
    let answer = block_on(board.get_at("HEAD", &["/tasks/a"]));
    std::fs::remove_dir_all(directory).unwrap();
    match answer {
        Err(Failure::Board(failure)) => assert_eq!(failure.code, "board::cli::usage"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_child_past_its_deadline_fails_as_a_child_failure() {
    let limits = Limits {
        child_deadline: Duration::from_millis(300),
        ..Limits::default()
    };
    let (board, directory) = scripted("sleep 30\n", limits);
    let answer = block_on(board.log("/tasks/a", None, Rows::Newest));
    std::fs::remove_dir_all(directory).unwrap();
    assert!(
        matches!(&answer, Err(Failure::Child(text)) if text.contains("deadline")),
        "{answer:?}"
    );
}

#[test]
fn a_child_that_prints_past_the_output_limit_fails_as_a_child_failure() {
    let limits = Limits {
        output_bytes: 16,
        ..Limits::default()
    };
    let (board, directory) = scripted("printf '%0100d' 0\n", limits);
    let answer = block_on(board.get(&["/tasks/a"]));
    std::fs::remove_dir_all(directory).unwrap();
    assert!(
        matches!(&answer, Err(Failure::Child(text)) if text.contains("bytes")),
        "{answer:?}"
    );
}

#[test]
fn the_child_gets_only_the_configured_environment_and_the_paths_after_a_separator() {
    let (board, directory) = scripted(
        "printf '{\"schema\":\"board/ls-report/v2\",\"data\":[{\"kind\":\"record\",\"logical\":\"%s|%s|%s|%s\"}]}' \"$*\" \"$HOME\" \"$GIT_COMMITTER_EMAIL\" \"${USER:-unset}\"\n",
        Limits::default(),
    );
    let answer = block_on(board.ls("/tasks")).unwrap().unwrap();
    std::fs::remove_dir_all(&directory).unwrap();
    let row = answer.first().unwrap();
    assert_eq!(
        row.logical,
        format!(
            "ls --json --all -- /tasks|{}|hangar@example.invalid|unset",
            directory.display()
        )
    );
}

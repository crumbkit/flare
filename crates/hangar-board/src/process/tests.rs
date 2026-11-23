use super::{Board, Rows, ended};
use crate::Limits;
use crate::report::{Failure, Got};
use crate::wait::Gate;
use crumbkit_process::{Cancel, Outcome, Status};
use hangar_core::contract::block_on;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Waker};
use std::time::{Duration, Instant};

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
        committer: ("flare".into(), "flare@example.invalid".into()),
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
        "printf '{\"schema\":\"board/ls-report/v2\",\"data\":[{\"kind\":\"record\",\"logical\":\"%s|%s|%s|%s|%s\"}]}' \"$*\" \"$HOME\" \"$GIT_COMMITTER_EMAIL\" \"${GIT_CONFIG_NOSYSTEM:-unset}\" \"${USER:-unset}\"\n",
        Limits::default(),
    );
    let answer = block_on(board.ls("/tasks")).unwrap().unwrap();
    std::fs::remove_dir_all(&directory).unwrap();
    let row = answer.first().unwrap();
    assert_eq!(
        row.logical,
        format!(
            "ls --json --all -- /tasks|{}|flare@example.invalid|1|unset",
            directory.display()
        )
    );
}

#[test]
fn a_child_that_ends_past_the_output_limit_fails_as_a_child_failure() {
    let answer = ended(Outcome::Ended(Status::Code(0)), true, &Limits::default());
    assert!(
        matches!(&answer, Err(Failure::Child(text)) if text.contains("bytes")),
        "{answer:?}"
    );
}

#[test]
fn a_child_that_ends_within_the_output_limit_answers_its_exit_code() {
    let answer = ended(Outcome::Ended(Status::Code(1)), false, &Limits::default());
    assert_eq!(answer, Ok(1));
}

#[test]
fn a_child_that_a_signal_ended_fails_as_a_child_failure() {
    let answer = ended(Outcome::Ended(Status::Signal(9)), false, &Limits::default());
    assert!(
        matches!(&answer, Err(Failure::Child(text)) if text.contains("signal")),
        "{answer:?}"
    );
}

#[test]
fn a_failure_report_with_exit_status_0_fails_as_a_child_failure() {
    let (board, directory) = scripted(
        "printf '%s' '{\"schema\":\"board/failure/v1\",\"code\":\"board::path::not_found\",\"message\":\"gone\"}'\n",
        Limits::default(),
    );
    let answer = block_on(board.ls("/tasks"));
    std::fs::remove_dir_all(directory).unwrap();
    assert!(
        matches!(&answer, Err(Failure::Child(text)) if text.contains("exit")),
        "{answer:?}"
    );
}

#[test]
fn a_report_that_lists_no_failures_with_exit_status_1_fails_as_a_child_failure() {
    let (board, directory) = scripted(
        "printf '%s' '{\"schema\":\"board/ls-report/v2\",\"data\":[]}'\nexit 1\n",
        Limits::default(),
    );
    let answer = block_on(board.ls("/tasks"));
    std::fs::remove_dir_all(directory).unwrap();
    assert!(
        matches!(&answer, Err(Failure::Child(text)) if text.contains("exit")),
        "{answer:?}"
    );
}

#[test]
fn a_get_report_that_lists_failures_may_exit_with_status_1() {
    let (board, directory) = scripted(
        "printf '%s' '{\"schema\":\"board/get-report/v2\",\"data\":[],\"failures\":[{\"index\":0,\"code\":\"board::path::not_found\"}]}'\nexit 1\n",
        Limits::default(),
    );
    let answer = block_on(board.get(&["/tasks/a"]));
    std::fs::remove_dir_all(directory).unwrap();
    assert_eq!(answer, Ok(vec![Got::Missing]));
}

#[test]
fn a_call_that_finds_no_free_place_before_the_deadline_fails_as_a_child_failure() {
    let limits = Limits {
        children: 1,
        child_deadline: Duration::from_millis(300),
        ..Limits::default()
    };
    let (board, directory) = scripted(
        "printf '%s' '{\"schema\":\"board/ls-report/v2\",\"data\":[]}'\n",
        limits,
    );
    let held = board
        .gate
        .enter_within(Instant::now() + Duration::from_secs(5), &Cancel::new())
        .unwrap();
    let started = Instant::now();
    let answer = block_on(board.ls("/tasks"));
    let took = started.elapsed();
    drop(held);
    std::fs::remove_dir_all(directory).unwrap();
    assert!(
        matches!(&answer, Err(Failure::Child(text)) if text.contains("place")),
        "{answer:?}"
    );
    assert!(took < Duration::from_secs(5), "the call waited {took:?}");
}

/// Polls `future` once, which starts its thread.
fn start<F: Future>(future: &mut Pin<Box<F>>) {
    let mut context = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut context).is_pending());
}

/// Lets another thread or the child run for `millis`.
fn pause(millis: u64) {
    #[expect(
        clippy::disallowed_methods,
        reason = "the test gives a thread or a child time to act, which shows no signal"
    )]
    std::thread::sleep(Duration::from_millis(millis));
}

/// Waits until `path` exists, for at most ten seconds.
fn wait_for(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "{} never appeared",
            path.display()
        );
        pause(10);
    }
}

#[test]
fn dropping_a_call_stops_its_child_and_returns_once_the_child_has_ended() {
    let (board, directory) = scripted(
        "trap 'sleep 0.3; echo stopped > stopped; exit 1' TERM\necho started > started\nsleep 30 &\nwait\n",
        Limits::default(),
    );
    let mut call = Box::pin(board.put_batch(&[]));
    start(&mut call);
    wait_for(&directory.join("started"));
    drop(call);
    let stopped = directory.join("stopped").exists();
    std::fs::remove_dir_all(directory).unwrap();
    assert!(stopped, "the child still ran when the drop returned");
}

#[test]
fn dropping_a_call_that_waits_for_a_place_gives_up_the_wait() {
    let limits = Limits {
        children: 1,
        ..Limits::default()
    };
    let (board, directory) = scripted("echo ran > ran\n", limits);
    let held = board
        .gate
        .enter_within(Instant::now() + Duration::from_secs(5), &Cancel::new())
        .unwrap();
    let mut call = Box::pin(board.ls("/tasks"));
    start(&mut call);
    pause(100);
    let started = Instant::now();
    drop(call);
    let took = started.elapsed();
    drop(held);
    pause(300);
    let ran = directory.join("ran").exists();
    std::fs::remove_dir_all(directory).unwrap();
    assert!(!ran, "the dropped call still ran its child");
    assert!(took < Duration::from_secs(2), "the drop waited {took:?}");
}

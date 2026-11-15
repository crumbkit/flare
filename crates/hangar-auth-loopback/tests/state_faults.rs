//! Faults in the state directory that timing alone does not produce: a file
//! replaced between the check and the open, a rename over the open file, a
//! FIFO, a file where the directory should be, and another owner. The
//! `testing` feature gives a hook between the steps of a read and a way to
//! read as another user id.
//!
//! Faults these tests do not inject, and why: a poisoned table lock needs a
//! panic inside the crate's own lock; no random bytes needs a failing
//! operating system source; and a failed write, rename or directory sync in
//! `write_server` needs a full or read-only file system. Each of those paths
//! is a single `map_err` in the code.
#![cfg(test)]

#[path = "support/fixture.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the fixture is test support: it panics on a broken fixture, and each test file uses part of it"
)]
pub mod fixture;
#[path = "support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch directory is test support: it panics on a broken fixture, and each test file uses part of it"
)]
pub mod scratch;

use fixture::loopback;
use hangar_auth_loopback::{ReadStep, ServerRecord, StateDir, StateError, Untrusted};
use scratch::{Scratch, write};
use secrecy::ExposeSecret as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

/// How long the FIFO test waits for `read_server` to answer before it
/// counts the read as hung.
const HUNG: Duration = Duration::from_secs(10);

fn prepared(scratch: &Scratch) -> StateDir {
    let state = StateDir::for_board(scratch.path(), Path::new("/boards/one"));
    state.prepare(SystemTime::now()).unwrap();
    state
}

fn same(left: &ServerRecord, right: &ServerRecord) -> bool {
    left.launch_host() == right.launch_host()
        && left.token().expose_secret() == right.token().expose_secret()
}

fn reason(error: StateError) -> Untrusted {
    match error {
        StateError::Untrusted { reason, .. } => reason,
        other => panic!("expected Untrusted, got {other:?}"),
    }
}

/// A hook that counts the reads that reach `step` and runs `fault` on the
/// first `times` of them.
fn hook(
    step: ReadStep,
    times: usize,
    calls: &Arc<AtomicUsize>,
    fault: impl Fn() + Send + Sync + 'static,
) -> impl Fn(ReadStep, &Path) + Send + Sync + 'static {
    let calls = Arc::clone(calls);
    move |reached, _path| {
        if reached == step && calls.fetch_add(1, Ordering::SeqCst) < times {
            fault();
        }
    }
}

#[test]
fn a_file_replaced_before_every_open_is_refused_as_replaced_after_three_tries() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    state.write_server(&loopback().server_record()).unwrap();
    let writer = state.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let reader = state.with_read_hook(hook(ReadStep::Checked, usize::MAX, &calls, move || {
        writer.write_server(&loopback().server_record()).unwrap();
    }));
    assert_eq!(
        reason(reader.read_server().unwrap_err()),
        Untrusted::Replaced
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[test]
fn a_file_replaced_once_before_the_open_is_read_on_the_retry() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    state.write_server(&loopback().server_record()).unwrap();
    let newer = loopback().server_record();
    let (writer, swapped_in) = (state.clone(), newer.clone());
    let calls = Arc::new(AtomicUsize::new(0));
    let reader = state.with_read_hook(hook(ReadStep::Checked, 1, &calls, move || {
        writer.write_server(&swapped_in).unwrap();
    }));
    let read = reader.read_server().unwrap().unwrap();
    assert!(same(&read, &newer));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn a_rename_over_the_open_file_is_accepted_with_0_links_and_gives_the_whole_old_record() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    let old = loopback().server_record();
    let newer = loopback().server_record();
    state.write_server(&old).unwrap();
    let (writer, swapped_in) = (state.clone(), newer.clone());
    let calls = Arc::new(AtomicUsize::new(0));
    // The rename lands after the open and before the open file is checked,
    // so the file the read holds has no name left: 0 links.
    let reader = state
        .clone()
        .with_read_hook(hook(ReadStep::Opened, 1, &calls, move || {
            writer.write_server(&swapped_in).unwrap();
        }));
    let read = reader.read_server().unwrap().unwrap();
    assert!(same(&read, &old), "the read gives the record it opened");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(same(&state.read_server().unwrap().unwrap(), &newer));
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the test makes a FIFO with /usr/bin/mkfifo, because making one directly needs unsafe code"
)]
fn a_fifo_put_in_place_of_the_file_before_the_open_is_refused_without_waiting() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    state.write_server(&loopback().server_record()).unwrap();
    let file = state.path().join("server.json");
    let calls = Arc::new(AtomicUsize::new(0));
    let reader = state.with_read_hook(hook(ReadStep::Checked, 1, &calls, move || {
        std::fs::remove_file(&file).unwrap();
        let status = std::process::Command::new("/usr/bin/mkfifo")
            .arg(&file)
            .status()
            .unwrap();
        assert!(status.success());
    }));
    let (send, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = send.send(reader.read_server().map(|_| ()));
    });
    let read = answer
        .recv_timeout(HUNG)
        .expect("read_server hung on a FIFO");
    assert_eq!(reason(read.unwrap_err()), Untrusted::NotRegular);
}

#[test]
fn a_regular_file_in_place_of_the_directory_is_refused_as_not_a_directory() {
    let scratch = Scratch::new();
    let state = StateDir::for_board(scratch.path(), Path::new("/boards/one"));
    std::fs::create_dir_all(state.path().parent().unwrap()).unwrap();
    write(state.path(), b"not a directory");
    assert_eq!(
        reason(state.prepare(SystemTime::now()).unwrap_err()),
        Untrusted::NotDirectory
    );
    assert_eq!(
        reason(state.read_server().unwrap_err()),
        Untrusted::NotDirectory
    );
}

#[test]
fn a_directory_and_file_of_another_user_are_refused_as_owner() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    state.write_server(&loopback().server_record()).unwrap();
    let mine = std::fs::metadata(state.path()).unwrap().uid();
    let other = state.clone().as_user(mine + 1);
    assert_eq!(reason(other.read_server().unwrap_err()), Untrusted::Owner);
    assert_eq!(
        reason(other.prepare(SystemTime::now()).unwrap_err()),
        Untrusted::Owner
    );
    assert!(state.read_server().unwrap().is_some());
}

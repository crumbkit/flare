//! The state directory: where it is, the server file and the openers, and
//! what the crate refuses to trust.
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
use hangar_auth_loopback::{ServerRecord, StateDir, StateError, Untrusted, state_home};
use scratch::{Scratch, chmod, mode, write};
use secrecy::{ExposeSecret as _, SecretString};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

fn prepared(scratch: &Scratch) -> StateDir {
    let state = StateDir::for_board(scratch.path(), Path::new("/boards/one"));
    state.prepare(SystemTime::now()).unwrap();
    state
}

fn same(left: &ServerRecord, right: &ServerRecord) -> bool {
    left.launch_host() == right.launch_host()
        && left.token().expose_secret() == right.token().expose_secret()
}

fn untrusted(error: StateError) -> Untrusted {
    match error {
        StateError::Untrusted { reason, .. } => reason,
        other => panic!("expected Untrusted, got {other:?}"),
    }
}

#[test]
fn the_state_home_is_an_absolute_xdg_state_home() {
    assert_eq!(
        state_home(Some(OsStr::new("/x/state")), Some(OsStr::new("/home/ada"))).unwrap(),
        PathBuf::from("/x/state")
    );
}

#[test]
fn the_state_home_falls_back_to_home_for_an_unset_empty_or_relative_xdg_state_home() {
    for xdg in [None, Some(OsStr::new("")), Some(OsStr::new("state"))] {
        assert_eq!(
            state_home(xdg, Some(OsStr::new("/home/ada"))).unwrap(),
            PathBuf::from("/home/ada/.local/state"),
            "{xdg:?}"
        );
    }
}

#[test]
fn the_state_home_is_refused_when_neither_variable_is_usable() {
    for home in [None, Some(OsStr::new("")), Some(OsStr::new("home"))] {
        assert!(
            matches!(state_home(None, home), Err(StateError::NoStateHome)),
            "{home:?}"
        );
    }
}

#[test]
fn prepare_makes_a_directory_of_mode_0700_under_hangar_loopback() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    assert!(
        state
            .path()
            .starts_with(scratch.path().join("hangar/loopback"))
    );
    let key = state.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(key.len(), 32);
    assert_eq!(mode(state.path()), 0o700);
}

#[test]
fn two_boards_get_two_directories() {
    let scratch = Scratch::new();
    let one = StateDir::for_board(scratch.path(), Path::new("/boards/one"));
    let two = StateDir::for_board(scratch.path(), Path::new("/boards/two"));
    assert_ne!(one.path(), two.path());
}

#[test]
fn write_server_makes_a_file_of_mode_0600_that_read_server_returns() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    let record = loopback().server_record();
    state.write_server(&record).unwrap();
    assert_eq!(mode(&state.path().join("server.json")), 0o600);
    let read = state.read_server().unwrap().unwrap();
    assert!(same(&read, &record));
}

#[test]
fn a_missing_directory_or_file_reads_as_none() {
    let scratch = Scratch::new();
    let state = StateDir::for_board(scratch.path(), Path::new("/boards/one"));
    assert!(state.read_server().unwrap().is_none());
    state.prepare(SystemTime::now()).unwrap();
    assert!(state.read_server().unwrap().is_none());
}

#[test]
fn read_server_refuses_a_file_of_mode_0644_until_it_is_put_right() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    state.write_server(&loopback().server_record()).unwrap();
    let file = state.path().join("server.json");
    chmod(&file, 0o644);
    assert_eq!(
        untrusted(state.read_server().unwrap_err()),
        Untrusted::Mode(0o644)
    );
    chmod(&file, 0o600);
    assert!(state.read_server().unwrap().is_some());
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the test moves the server file aside and back to put a link in its place"
)]
fn read_server_refuses_a_link_to_a_good_file_until_it_is_put_right() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    state.write_server(&loopback().server_record()).unwrap();
    let file = state.path().join("server.json");
    let elsewhere = scratch.path().join("good.json");
    std::fs::rename(&file, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &file).unwrap();
    assert_eq!(untrusted(state.read_server().unwrap_err()), Untrusted::Link);
    std::fs::remove_file(&file).unwrap();
    std::fs::rename(&elsewhere, &file).unwrap();
    assert!(state.read_server().unwrap().is_some());
}

#[test]
fn read_server_refuses_a_socket_in_place_of_the_file_until_it_is_put_right() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    let file = state.path().join("server.json");
    let listener = std::os::unix::net::UnixListener::bind(&file).unwrap();
    assert_eq!(
        untrusted(state.read_server().unwrap_err()),
        Untrusted::NotRegular
    );
    drop(listener);
    std::fs::remove_file(&file).unwrap();
    state.write_server(&loopback().server_record()).unwrap();
    assert!(state.read_server().unwrap().is_some());
}

#[test]
fn read_server_refuses_a_file_with_a_second_link_until_it_is_put_right() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    state.write_server(&loopback().server_record()).unwrap();
    let file = state.path().join("server.json");
    let second = scratch.path().join("second.json");
    std::fs::hard_link(&file, &second).unwrap();
    assert_eq!(
        untrusted(state.read_server().unwrap_err()),
        Untrusted::Links(2)
    );
    std::fs::remove_file(&second).unwrap();
    assert!(state.read_server().unwrap().is_some());
}

#[test]
fn read_server_refuses_a_directory_of_mode_0755_until_it_is_put_right() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    state.write_server(&loopback().server_record()).unwrap();
    chmod(state.path(), 0o755);
    assert_eq!(
        untrusted(state.read_server().unwrap_err()),
        Untrusted::Mode(0o755)
    );
    chmod(state.path(), 0o700);
    assert!(state.read_server().unwrap().is_some());
}

#[test]
fn an_untrusted_refusal_names_the_path_and_the_reason() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    chmod(state.path(), 0o755);
    let message = state.read_server().unwrap_err().to_string();
    assert!(
        message.contains(&state.path().display().to_string()),
        "{message}"
    );
    assert!(message.contains("0755"), "{message}");
}

#[test]
fn prepare_refuses_a_directory_of_mode_0755_and_leaves_its_mode() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    chmod(state.path(), 0o755);
    assert_eq!(
        untrusted(state.prepare(SystemTime::now()).unwrap_err()),
        Untrusted::Mode(0o755)
    );
    assert_eq!(mode(state.path()), 0o755);
}

#[test]
fn prepare_refuses_a_link_to_a_directory_and_leaves_the_link() {
    let scratch = Scratch::new();
    let state = StateDir::for_board(scratch.path(), Path::new("/boards/one"));
    let target = scratch.path().join("real");
    std::fs::create_dir(&target).unwrap();
    chmod(&target, 0o700);
    std::fs::create_dir_all(state.path().parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, state.path()).unwrap();
    assert_eq!(
        untrusted(state.prepare(SystemTime::now()).unwrap_err()),
        Untrusted::Link
    );
    assert!(
        std::fs::symlink_metadata(state.path())
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn a_reader_during_a_hundred_writes_always_reads_a_whole_record() {
    let scratch = Scratch::new();
    let state = Arc::new(prepared(&scratch));
    let records = Arc::new([loopback().server_record(), loopback().server_record()]);
    state.write_server(&records[0]).unwrap();
    let done = Arc::new(AtomicBool::new(false));
    let writer = {
        let (state, records, done) = (Arc::clone(&state), Arc::clone(&records), Arc::clone(&done));
        #[expect(
            clippy::disallowed_methods,
            reason = "the test joins the writer thread"
        )]
        std::thread::spawn(move || {
            for round in 0..100 {
                state.write_server(&records[round % 2]).unwrap();
            }
            done.store(true, Ordering::SeqCst);
        })
    };
    let mut reads = 0;
    while !done.load(Ordering::SeqCst) || reads == 0 {
        let read = state.read_server().unwrap().unwrap();
        assert!(same(&read, &records[0]) || same(&read, &records[1]));
        reads += 1;
    }
    writer.join().unwrap();
}

#[test]
fn a_newer_version_is_refused_with_the_upgrade_message() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    let file = state.path().join("server.json");
    write(
        &file,
        br#"{"version":2,"host":"h-x.localhost","port":1,"token":"x"}"#,
    );
    chmod(&file, 0o600);
    let error = state.read_server().unwrap_err();
    assert!(
        matches!(error, StateError::Newer { version: 2, .. }),
        "{error:?}"
    );
    assert!(error.to_string().contains("upgrade hangar"), "{error}");
}

#[test]
fn malformed_json_and_a_bad_field_are_malformed() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    let file = state.path().join("server.json");
    let token = "a".repeat(64);
    let host = format!("h-{}.localhost", "b".repeat(32));
    for body in [
        "{not json".to_owned(),
        format!(r#"{{"version":1,"host":"evil.example","port":4100,"token":"{token}"}}"#),
        format!(r#"{{"version":1,"host":"{host}","port":0,"token":"{token}"}}"#),
        format!(r#"{{"version":1,"host":"{host}","port":4100,"token":"short"}}"#),
    ] {
        write(&file, body.as_bytes());
        chmod(&file, 0o600);
        assert!(
            matches!(state.read_server(), Err(StateError::Malformed { .. })),
            "{body}"
        );
    }
    write(
        &file,
        format!(r#"{{"version":1,"host":"{host}","port":4100,"token":"{token}"}}"#).as_bytes(),
    );
    assert!(state.read_server().unwrap().is_some());
}

#[test]
fn write_server_replaces_a_malformed_or_untrusted_file() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    let file = state.path().join("server.json");
    write(&file, b"{not json");
    chmod(&file, 0o644);
    state.write_server(&loopback().server_record()).unwrap();
    assert_eq!(mode(&file), 0o600);
    assert!(state.read_server().unwrap().is_some());
}

#[test]
fn remove_server_removes_the_file_of_its_own_server() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    let own = loopback().server_record();
    state.write_server(&own).unwrap();
    state.remove_server(&own).unwrap();
    assert!(state.read_server().unwrap().is_none());
}

#[test]
fn remove_server_leaves_the_file_of_a_server_that_replaced_it() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    let own = loopback().server_record();
    let newer = loopback().server_record();
    state.write_server(&own).unwrap();
    state.write_server(&newer).unwrap();
    state.remove_server(&own).unwrap();
    assert!(same(&state.read_server().unwrap().unwrap(), &newer));
}

#[test]
fn the_sweep_removes_old_temporary_files_and_openers_and_leaves_the_rest() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    let now = SystemTime::now();
    let names = [
        ".server.json.4242.0.tmp",
        "open-0123456789abcdef.html",
        ".server.json.4242.1.tmp",
        "open-fedcba9876543210.html",
        "notes.txt",
    ];
    for name in names {
        write(&state.path().join(name), b"x");
    }
    for name in [names[0], names[1], names[4]] {
        let file = std::fs::File::open(state.path().join(name)).unwrap();
        file.set_modified(now - Duration::from_secs(61)).unwrap();
    }
    state.prepare(now).unwrap();
    let left: Vec<bool> = names
        .iter()
        .map(|name| state.path().join(name).exists())
        .collect();
    assert_eq!(left, [false, false, true, true, true]);
}

#[test]
fn write_opener_writes_a_new_file_of_mode_0600_each_time() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    let page = SecretString::from("<!doctype html>");
    let first = state.write_opener(&page).unwrap();
    let second = state.write_opener(&page).unwrap();
    assert_ne!(first, second);
    for opener in [&first, &second] {
        assert_eq!(opener.parent(), Some(state.path()));
        let name = opener.file_name().unwrap().to_str().unwrap();
        assert!(
            name.starts_with("open-") && name.ends_with(".html"),
            "{name}"
        );
        assert_eq!(name.len(), "open-.html".len() + 16);
        assert_eq!(mode(opener), 0o600);
        assert_eq!(std::fs::read_to_string(opener).unwrap(), "<!doctype html>");
    }
}

#[test]
fn remove_opener_accepts_a_file_already_gone() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    let opener = state.write_opener(&SecretString::from("x")).unwrap();
    state.remove_opener(&opener).unwrap();
    assert!(!opener.exists());
    state.remove_opener(&opener).unwrap();
}

#[test]
fn remove_opener_refuses_a_file_that_is_not_an_opener_of_this_directory() {
    let scratch = Scratch::new();
    let state = prepared(&scratch);
    let other = scratch.path().join("open-0123456789abcdef.html");
    write(&other, b"x");
    let server = state.path().join("server.json");
    for path in [other.as_path(), server.as_path()] {
        assert!(
            matches!(state.remove_opener(path), Err(StateError::NotOpener { .. })),
            "{}",
            path.display()
        );
    }
    assert!(other.exists());
}

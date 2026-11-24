use super::{
    MAX_BYTES, MAX_LABEL_BYTES, StateFile, label, load, read_capped, remember_label, store, write,
};
use crate::codes::Failed;
use crate::env::Snapshot;
use crumbkit::{Failure as _, Field};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[path = "../../../../flare-auth-loopback/tests/support/scratch.rs"]
#[expect(
    dead_code,
    unreachable_pub,
    reason = "the scratch directory is flare-auth-loopback's test support, and these tests use part of it"
)]
mod scratch;

use scratch::{Scratch, mode};

/// The message and the fields of `failed`.
fn shown(failed: &Failed) -> (String, Vec<Field<'_>>) {
    (failed.to_string(), failed.fields())
}

/// A snapshot whose state home is `scratch`, and flare's directory in it.
fn state(scratch: &Scratch) -> (Snapshot, PathBuf) {
    let env = Snapshot::with_var("XDG_STATE_HOME", scratch.path());
    (env, scratch.path().join("flare"))
}

/// Writes `bytes` to `file`, creating its directory first.
fn put(file: &std::path::Path, bytes: &[u8]) {
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    scratch::write(file, bytes);
}

/// A saved login, for the tests that store one.
fn saved() -> super::Saved {
    super::Saved {
        server: "https://flare.example".to_owned(),
        token_id: "token".to_owned(),
        label: "label".to_owned(),
        owner: "owner@example.com".to_owned(),
    }
}

#[test]
fn a_login_file_longer_than_4096_bytes_is_refused_with_a_message_that_starts_with_its_path() {
    let scratch = Scratch::new();
    let (env, flare) = state(&scratch);
    let file = flare.join("login.json");
    put(&file, &vec![b' '; 4097]);
    let name = file.display().to_string();
    assert_eq!(
        shown(&load(&env).unwrap_err()),
        (
            format!("the saved login {name} could not be used: it is longer than 4096 bytes"),
            vec![Field::new("path", name.as_str())]
        )
    );
    scratch::write(&file, &vec![b' '; 4096]);
    assert_eq!(
        read_capped(StateFile::Login, &file).unwrap(),
        Some(" ".repeat(4096))
    );
    assert_eq!(MAX_BYTES, 4096);
}

#[test]
fn a_login_file_that_cannot_be_read_is_refused_with_a_message_that_starts_with_its_path() {
    let scratch = Scratch::new();
    let (env, flare) = state(&scratch);
    let file = flare.join("login.json");
    std::fs::create_dir_all(&file).unwrap();
    let name = file.display().to_string();
    assert_eq!(
        shown(&load(&env).unwrap_err()),
        (
            format!("the saved login {name} could not be used: Is a directory (os error 21)"),
            vec![Field::new("path", name.as_str())]
        )
    );
    std::fs::remove_dir(&file).unwrap();
    assert_eq!(load(&env).unwrap(), None);
}

#[test]
fn a_login_file_that_is_not_json_is_refused_with_a_message_that_starts_with_its_path() {
    let scratch = Scratch::new();
    let (env, flare) = state(&scratch);
    let file = flare.join("login.json");
    put(&file, b"not json");
    let name = file.display().to_string();
    assert_eq!(
        shown(&load(&env).unwrap_err()),
        (
            format!("the saved login {name} could not be used: expected ident at line 1 column 2"),
            vec![Field::new("path", name.as_str())]
        )
    );
}

#[test]
fn a_labels_file_that_is_not_json_is_refused_as_the_login_labels_file_and_not_the_saved_login() {
    let scratch = Scratch::new();
    let (env, flare) = state(&scratch);
    let file = flare.join("labels.json");
    put(&file, b"not json");
    let name = file.display().to_string();
    let failed = label(&env, "https://flare.example").unwrap_err();
    assert_eq!(
        shown(&failed),
        (
            format!(
                "the login labels file {name} could not be used: expected ident at line 1 column 2"
            ),
            vec![Field::new("path", name.as_str())]
        )
    );
    assert!(!failed.to_string().contains("saved login"), "{failed}");
}

#[test]
fn a_labels_file_longer_than_64_kib_is_refused_with_a_message_that_starts_with_its_path() {
    let scratch = Scratch::new();
    let (env, flare) = state(&scratch);
    let file = flare.join("labels.json");
    put(&file, &vec![b' '; 64 * 1024 + 1]);
    let name = file.display().to_string();
    assert_eq!(
        shown(&label(&env, "https://flare.example").unwrap_err()),
        (
            format!(
                "the login labels file {name} could not be used: it is longer than 65536 bytes"
            ),
            vec![Field::new("path", name.as_str())]
        )
    );
}

#[test]
fn a_label_that_would_take_the_labels_file_past_64_kib_is_refused_with_its_path_and_the_file_kept()
{
    let scratch = Scratch::new();
    let (env, flare) = state(&scratch);
    let file = flare.join("labels.json");
    let empty = serde_json::to_vec_pretty(&BTreeMap::from([("a", "")]))
        .unwrap()
        .len();
    let full = BTreeMap::from([(
        "a".to_owned(),
        "x".repeat(usize::try_from(MAX_LABEL_BYTES).unwrap() - empty),
    )]);
    let bytes = serde_json::to_vec_pretty(&full).unwrap();
    assert_eq!(u64::try_from(bytes.len()).unwrap(), MAX_LABEL_BYTES);
    put(&file, &bytes);
    let name = file.display().to_string();
    assert_eq!(
        shown(&remember_label(&env, "https://flare.example", "label").unwrap_err()),
        (
            format!("the login labels file {name} could not be used: it would pass 65536 bytes"),
            vec![Field::new("path", name.as_str())]
        )
    );
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
}

#[test]
fn a_state_directory_that_cannot_be_created_is_named_at_the_start_of_the_message() {
    let scratch = Scratch::new();
    let (env, flare) = state(&scratch);
    scratch::write(&flare, b"a file where flare wants a directory");
    let name = flare.display().to_string();
    let expected = (
        format!("the state directory {name} could not be created: File exists (os error 17)"),
        vec![Field::new("path", name.as_str())],
    );
    assert_eq!(shown(&store(&env, &saved()).unwrap_err()), expected);
    assert_eq!(
        shown(&write(&flare.join("labels.json"), StateFile::Labels, b"{}").unwrap_err()),
        expected
    );
    std::fs::remove_file(&flare).unwrap();
    store(&env, &saved()).unwrap();
    assert_eq!(mode(&flare.join("login.json")), 0o600);
    assert_eq!(load(&env).unwrap(), Some(saved()));
}

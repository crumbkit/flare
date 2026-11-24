use super::{read_capped, write};
use crate::codes::Failed;
use crumbkit::{Failure as _, Field};

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

#[test]
fn a_file_longer_than_its_limit_is_refused_naming_the_file_in_the_path_field() {
    let scratch = Scratch::new();
    let file = scratch.path().join("login.json");
    scratch::write(&file, b"123456789");
    let name = file.display().to_string();
    assert_eq!(
        shown(&read_capped(&file, 8).unwrap_err()),
        (
            format!("the saved login could not be used: {name} is longer than 8 bytes"),
            vec![Field::new("path", name.as_str())]
        )
    );
    scratch::write(&file, b"12345678");
    assert_eq!(read_capped(&file, 8).unwrap(), Some("12345678".to_owned()));
}

#[test]
fn a_file_that_cannot_be_read_is_refused_naming_it_in_the_path_field() {
    let scratch = Scratch::new();
    let name = scratch.path().display().to_string();
    assert_eq!(
        shown(&read_capped(scratch.path(), 8).unwrap_err()),
        (
            "the saved login could not be used: Is a directory (os error 21)".to_owned(),
            vec![Field::new("path", name.as_str())]
        )
    );
    assert_eq!(
        read_capped(&scratch.path().join("login.json"), 8).unwrap(),
        None
    );
}

#[test]
fn a_directory_that_cannot_be_made_is_named_in_the_path_field() {
    let scratch = Scratch::new();
    let plain = scratch.path().join("plain");
    scratch::write(&plain, b"a file where flare wants a directory");
    let name = plain.display().to_string();
    assert_eq!(
        shown(&write(&plain.join("labels.json"), b"{}").unwrap_err()),
        (
            "the saved login could not be used: File exists (os error 17)".to_owned(),
            vec![Field::new("path", name.as_str())]
        )
    );
    let file = scratch.path().join("flare").join("labels.json");
    write(&file, b"{}").unwrap();
    assert_eq!(mode(&file), 0o600);
}

//! `FileSecret` against files in a scratch directory: the files it reads,
//! a symbolic link as a Kubernetes secret mount presents one, and each kind
//! of file it refuses beside a file it reads.
#![cfg(test)]
#![cfg(unix)]

use flare_auth_oidc::{ClientSecretSource, FileSecret, SecretError};
use secrecy::ExposeSecret as _;
use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

const SECRET: &str = "made-up-client-secret-c41d";

/// A new empty directory for one test.
fn scratch(name: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("file-secret-{name}"));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    directory
}

/// Writes `contents` to `name` in `directory` with `mode`.
#[expect(
    clippy::disallowed_methods,
    reason = "a test writes its fixture file directly, outside the atomic writer"
)]
fn file(directory: &Path, name: &str, contents: &[u8], mode: u32) -> PathBuf {
    let path = directory.join(name);
    fs::write(&path, contents).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    path
}

fn read(path: &Path) -> Result<String, SecretError> {
    FileSecret::new(path)
        .client_secret()
        .map(|secret| secret.expose_secret().to_owned())
}

fn running_as_root() -> bool {
    let directory = scratch("whoami");
    let probe = file(&directory, "probe", b"x", 0o600);
    fs::metadata(probe).unwrap().uid() == 0
}

#[test]
fn a_0600_file_of_the_running_user_is_read_with_one_trailing_newline_trimmed() {
    let directory = scratch("plain");
    let path = file(
        &directory,
        "secret",
        format!("{SECRET}\n").as_bytes(),
        0o600,
    );
    assert_eq!(read(&path).unwrap(), SECRET);
    let two = file(&directory, "two", format!("{SECRET}\n\n").as_bytes(), 0o600);
    assert_eq!(read(&two).unwrap(), format!("{SECRET}\n"));
    let none = file(&directory, "none", SECRET.as_bytes(), 0o400);
    assert_eq!(read(&none).unwrap(), SECRET);
}

#[test]
fn a_symbolic_link_to_a_good_file_is_followed_as_a_secret_mount_presents_it() {
    let directory = scratch("mount");
    let data = directory.join("..2026_11_19_00_00_00.000000000");
    fs::create_dir_all(&data).unwrap();
    file(&data, "client_secret", SECRET.as_bytes(), 0o600);
    std::os::unix::fs::symlink(&data, directory.join("..data")).unwrap();
    let link = directory.join("client_secret");
    std::os::unix::fs::symlink("..data/client_secret", &link).unwrap();
    assert_eq!(read(&link).unwrap(), SECRET);
}

#[test]
fn a_world_readable_0644_file_is_read() {
    let directory = scratch("readable");
    let path = file(&directory, "secret", SECRET.as_bytes(), 0o644);
    assert_eq!(read(&path).unwrap(), SECRET);
}

#[test]
fn a_group_or_world_writable_file_is_refused_and_the_same_file_made_private_is_read() {
    let directory = scratch("writable");
    for mode in [0o620, 0o602, 0o666] {
        let path = file(&directory, "secret", SECRET.as_bytes(), mode);
        assert_eq!(read(&path), Err(SecretError::FileWritable), "{mode:o}");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read(&path).unwrap(), SECRET, "{mode:o}");
    }
}

#[test]
fn a_directory_is_refused_and_a_file_in_it_is_read() {
    let directory = scratch("directory");
    assert_eq!(read(&directory), Err(SecretError::NotAFile));
    let path = file(&directory, "secret", SECRET.as_bytes(), 0o600);
    assert_eq!(read(&path).unwrap(), SECRET);
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the test makes its FIFO with mkfifo, because the crate forbids the unsafe libc call"
)]
fn a_fifo_is_refused_without_blocking_and_a_file_in_its_place_is_read() {
    let directory = scratch("fifo");
    let fifo = directory.join("secret");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(made.success());
    assert_eq!(read(&fifo), Err(SecretError::NotAFile));
    fs::remove_file(&fifo).unwrap();
    let path = file(&directory, "secret", SECRET.as_bytes(), 0o600);
    assert_eq!(read(&path).unwrap(), SECRET);
}

#[test]
fn a_missing_file_is_refused_and_a_present_one_is_read() {
    let directory = scratch("missing");
    let path = directory.join("secret");
    assert_eq!(read(&path), Err(SecretError::FileUnreadable));
    file(&directory, "secret", SECRET.as_bytes(), 0o600);
    assert_eq!(read(&path).unwrap(), SECRET);
}

#[test]
fn an_empty_file_is_refused_and_a_one_byte_file_is_read() {
    let directory = scratch("empty");
    let empty = file(&directory, "empty", b"", 0o600);
    assert_eq!(read(&empty), Err(SecretError::Empty));
    let newline = file(&directory, "newline", b"\n", 0o600);
    assert_eq!(read(&newline), Err(SecretError::Empty));
    let one = file(&directory, "one", b"x", 0o600);
    assert_eq!(read(&one).unwrap(), "x");
}

#[test]
fn a_4097_byte_file_is_refused_and_a_4096_byte_file_is_read() {
    let directory = scratch("long");
    let long = file(&directory, "long", &[b'x'; 4097], 0o600);
    assert_eq!(read(&long), Err(SecretError::TooLong));
    let most = file(&directory, "most", &[b'x'; 4096], 0o600);
    assert_eq!(read(&most).unwrap().len(), 4096);
}

#[test]
fn a_file_that_is_not_utf8_is_refused_and_a_utf8_one_is_read() {
    let directory = scratch("text");
    let bytes = file(&directory, "bytes", &[0xff, 0xfe, b'x'], 0o600);
    assert_eq!(read(&bytes), Err(SecretError::NotText));
    let text = file(&directory, "text", "caf\u{e9}".as_bytes(), 0o600);
    assert_eq!(read(&text).unwrap(), "caf\u{e9}");
}

#[test]
fn the_debug_output_names_the_path_and_never_the_secret() {
    let directory = scratch("debug");
    let path = file(&directory, "secret", SECRET.as_bytes(), 0o600);
    let source = FileSecret::new(&path);
    let secret = source.client_secret().unwrap();
    let shown = format!("{source:?} {secret:?}");
    assert!(shown.contains("secret"), "{shown}");
    assert!(!shown.contains(SECRET), "{shown}");
}

#[test]
fn a_root_owned_0400_file_is_read_when_the_tests_run_as_root() {
    if !running_as_root() {
        eprintln!("skipped: this test runs only as root, which can own a file as root");
        return;
    }
    let directory = scratch("root");
    let path = file(&directory, "secret", SECRET.as_bytes(), 0o400);
    assert_eq!(fs::metadata(&path).unwrap().uid(), 0);
    assert_eq!(read(&path).unwrap(), SECRET);
}

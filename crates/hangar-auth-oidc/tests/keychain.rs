//! `KeychainSecret` against fake `security` programs, which are shell
//! scripts that record what they got.
#![cfg(test)]
#![cfg(unix)]

use hangar_auth_oidc::{ClientSecretSource, KeychainSecret, SecretError};
use secrecy::ExposeSecret;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SERVICE: &str = "hangar-oidc-test";

/// Writes a script named `name` with `body` after its shebang line into a
/// directory of its own, and returns the script's path and the directory.
fn script(name: &str, body: &str) -> (PathBuf, PathBuf) {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("keychain-{name}"));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("security");
    let body = body.replace("DIR", directory.to_str().unwrap());
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    (path, directory)
}

fn read(program: PathBuf, deadline: Duration) -> Result<String, SecretError> {
    KeychainSecret::with_program(program, SERVICE, deadline)
        .unwrap()
        .client_secret()
        .map(|secret| secret.expose_secret().to_owned())
}

#[test]
fn the_program_gets_exactly_the_find_arguments_an_empty_environment_and_no_input() {
    let (program, directory) = script(
        "recorder",
        "printf '%s\\n' \"$@\" > DIR/args\n/usr/bin/env > DIR/env\n/bin/cat > DIR/stdin\nprintf 'the-secret\\n'",
    );
    assert_eq!(
        read(program, Duration::from_secs(10)),
        Ok("the-secret".to_owned())
    );
    let args = fs::read_to_string(directory.join("args")).unwrap();
    assert_eq!(args, format!("find-generic-password\n-s\n{SERVICE}\n-w\n"));
    let env = fs::read_to_string(directory.join("env")).unwrap();
    for inherited in ["HOME=", "USER=", "CARGO"] {
        assert!(
            !env.contains(inherited),
            "the child inherited {inherited}: {env}"
        );
    }
    assert_eq!(fs::read_to_string(directory.join("stdin")).unwrap(), "");
}

#[test]
fn the_secret_is_the_output_without_its_final_newline_and_other_newlines_stay() {
    let (program, _) = script("newline", "printf 'a b\\n\\n'");
    assert_eq!(
        read(program, Duration::from_secs(10)),
        Ok("a b\n".to_owned())
    );
    let (program, _) = script("no-newline", "printf 'plain'");
    assert_eq!(
        read(program, Duration::from_secs(10)),
        Ok("plain".to_owned())
    );
}

#[test]
fn a_missing_program_is_unavailable() {
    let missing = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("no-such-security");
    assert_eq!(
        read(missing, Duration::from_secs(10)),
        Err(SecretError::Unavailable)
    );
}

#[test]
fn a_non_zero_exit_is_a_failure() {
    let (program, _) = script("exit", "printf 'partial\\n'\nexit 44");
    assert_eq!(
        read(program, Duration::from_secs(10)),
        Err(SecretError::Failed)
    );
}

#[test]
fn empty_output_is_refused() {
    for (name, body) in [("empty", "exit 0"), ("only-newline", "printf '\\n'")] {
        let (program, _) = script(name, body);
        assert_eq!(
            read(program, Duration::from_secs(10)),
            Err(SecretError::Empty)
        );
    }
}

#[test]
fn output_over_4_kib_is_refused_the_child_is_reaped_and_4_kib_is_admitted() {
    // yes writes without end, so the child is still running when the
    // reader passes 4 KiB.
    let (program, directory) = script("long", "echo $$ > DIR/pid\nexec /usr/bin/yes");
    assert_eq!(
        read(program, Duration::from_secs(10)),
        Err(SecretError::TooLong)
    );
    let pid = fs::read_to_string(directory.join("pid")).unwrap();
    assert!(
        !process_exists(pid.trim()),
        "process {pid} is still running"
    );
    let fits = "x".repeat(4096);
    let (program, _) = script("fits", &format!("printf '{fits}'"));
    assert_eq!(read(program, Duration::from_secs(10)), Ok(fits));
}

#[test]
fn a_program_that_outlives_the_deadline_times_out_and_is_reaped() {
    let (program, directory) = script("slow", "echo $$ > DIR/pid\nexec /bin/sleep 30");
    let started = Instant::now();
    assert_eq!(
        read(program, Duration::from_millis(300)),
        Err(SecretError::TimedOut)
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    let pid = fs::read_to_string(directory.join("pid")).unwrap();
    assert!(
        !process_exists(pid.trim()),
        "process {pid} is still running"
    );
}

#[expect(
    clippy::disallowed_methods,
    reason = "the test asks kill whether the reaped child still exists"
)]
fn process_exists(pid: &str) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[test]
fn a_service_name_that_could_be_read_as_an_option_is_refused() {
    for service in ["", "-w", "a\nb"] {
        assert_eq!(
            KeychainSecret::new(service).map(|_| ()),
            Err(SecretError::Service)
        );
    }
    assert!(KeychainSecret::new("hangar-oidc-google").is_ok());
}

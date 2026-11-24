//! Saves, reads and deletes the CLI token in the macOS Keychain. This is
//! one of the command's two process modules (PROC-1); the other is
//! `opener.rs`. It is one of the modules that read the token (BASE-24), to
//! write it to the program's standard input.
//!
//! The token is the generic password of the service `flare-cli` and the
//! account that is the server's origin, so each server has its own item.
//!
//! PROC-2's five items:
//!
//! - **Program.** `FLARE_KEYCHAIN` when it is set, else
//!   `/usr/bin/security`, by that absolute path and never through `PATH`.
//!   The tests set `FLARE_KEYCHAIN` to a script that keeps the item in a
//!   file.
//! - **Arguments.** A save runs `security -i` and writes one command line,
//!   `add-generic-password -U -s flare-cli -a <origin> -w <token>`, to its
//!   standard input, so the token is never on a command line that another
//!   process can read. A read runs `find-generic-password -s flare-cli -a
//!   <origin> -w` and a delete runs `delete-generic-password -s flare-cli
//!   -a <origin>`, each as separate values with no shell. The origin and
//!   the token hold no space, quote or backslash, because the origin is a
//!   parsed URL origin and the token is `flare_` with hex and base64url
//!   characters; [`save`] refuses any other token.
//! - **Environment.** Empty. The program inherits no variable.
//! - **Streams and deadline.** A save's standard input is the one command
//!   line, which crumbkit-process clears from memory when it drops. A
//!   read captures at most 4 KiB of standard output, and the others
//!   discard it; standard error is discarded. Each run has 10 seconds.
//! - **End.** crumbkit-process starts the program in a process group of
//!   its own and reaps it. Past the deadline, or when a read's output
//!   passes 4 KiB, it sends the group SIGTERM, waits one second and kills
//!   the group. A status other than 0 is a failure, except that a delete
//!   of a missing item succeeds, because the item is gone either way.

use crate::codes::{Failed, SharingFailed};
use crate::env::Snapshot;
use crumbkit_process::{
    Capture, Escalation, Input, Outcome, RunOptions, Signal, Status, Zeroizing,
};
use secrecy::{ExposeSecret as _, SecretString};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

/// The Keychain service of every flare token.
const SERVICE: &str = "flare-cli";
const SECURITY: &str = "/usr/bin/security";
const DEADLINE: Duration = Duration::from_secs(10);
const MAX_OUTPUT: usize = 4096;
/// The status `security` exits with when no item matches.
const NOT_FOUND: i32 = 44;

fn failed(action: &'static str, problem: impl Into<String>) -> Failed {
    SharingFailed::KeychainUnavailable {
        action,
        problem: problem.into(),
    }
    .into()
}

fn program(env: &Snapshot) -> PathBuf {
    env.get("FLARE_KEYCHAIN")
        .map_or_else(|| PathBuf::from(SECURITY), PathBuf::from)
}

/// What one run of the program gave: its status and its output.
struct Ran {
    status: Option<i32>,
    output: Zeroizing<Vec<u8>>,
}

/// Runs the program with `args`, `input` on standard input, and, when
/// `capture` is set, its standard output kept.
#[expect(
    clippy::disallowed_methods,
    reason = "PROC-1: keychain.rs is one of the flare command's two process modules"
)]
fn run(
    env: &Snapshot,
    action: &'static str,
    args: &[&str],
    input: Input,
    capture: bool,
) -> Result<Ran, Failed> {
    let program = program(env);
    let mut command = Command::new(&program);
    command.args(args).env_clear();
    let deadline = Instant::now()
        .checked_add(DEADLINE)
        .ok_or_else(|| failed(action, "the deadline is too far away"))?;
    let stdout = if capture {
        Capture::whole(MAX_OUTPUT).into()
    } else {
        Capture::discard()
    };
    let options = RunOptions::new()
        .stdin(input)
        .stdout(stdout)
        .stderr(Capture::discard())
        .deadline(deadline)
        .escalation(Escalation::default().step(Signal::Terminate, Duration::from_secs(1)));
    let finished = crumbkit_process::run(command, &options).map_err(|error| {
        failed(
            action,
            format!("{} did not start: {error}", program.display()),
        )
    })?;
    let over_limit = finished.stdout().over_limit();
    let outcome = finished.outcome();
    let output = Zeroizing::new(finished.into_output().0.into_bytes());
    match outcome {
        Outcome::TimedOut => Err(failed(action, "the program ran past its 10 seconds")),
        _ if over_limit => Err(failed(action, "the program printed more than 4 KiB")),
        Outcome::Ended(Status::Code(code)) => Ok(Ran {
            status: Some(code),
            output,
        }),
        _ => Ok(Ran {
            status: None,
            output,
        }),
    }
}

/// Whether `token` holds only the characters a flare token has.
fn plain(token: &str) -> bool {
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// Saves `token` as the item of `origin`, replacing an older one.
///
/// # Errors
///
/// Returns [`SharingFailed::KeychainUnavailable`].
pub(crate) fn save(env: &Snapshot, origin: &str, token: &SecretString) -> Result<(), Failed> {
    if !plain(token.expose_secret()) {
        return Err(failed(
            "save",
            "the server's token holds characters a flare token never has",
        ));
    }
    let line = format!(
        "add-generic-password -U -s {SERVICE} -a {origin} -w {}\n",
        token.expose_secret()
    );
    let input = Input::Bytes(Zeroizing::new(line.into_bytes()));
    let ran = run(env, "save", &["-i"], input, false)?;
    match ran.status {
        Some(0) => Ok(()),
        other => Err(failed("save", format!("the program ended with {other:?}"))),
    }
}

/// Reads the token of `origin`, or `None` when the Keychain has none.
///
/// # Errors
///
/// Returns [`SharingFailed::KeychainUnavailable`].
pub(crate) fn read(env: &Snapshot, origin: &str) -> Result<Option<SecretString>, Failed> {
    let args = ["find-generic-password", "-s", SERVICE, "-a", origin, "-w"];
    let ran = run(env, "read", &args, Input::Null, true)?;
    match ran.status {
        Some(0) => {
            let text = std::str::from_utf8(&ran.output)
                .map_err(|_| failed("read", "the item is not text"))?
                .trim_end_matches('\n');
            if text.is_empty() {
                return Ok(None);
            }
            Ok(Some(SecretString::from(text.to_owned())))
        }
        Some(NOT_FOUND) => Ok(None),
        other => Err(failed("read", format!("the program ended with {other:?}"))),
    }
}

/// Deletes the token of `origin`. A missing item counts as deleted.
///
/// # Errors
///
/// Returns [`SharingFailed::KeychainUnavailable`].
pub(crate) fn delete(env: &Snapshot, origin: &str) -> Result<(), Failed> {
    let args = ["delete-generic-password", "-s", SERVICE, "-a", origin];
    let ran = run(env, "delete", &args, Input::Null, false)?;
    match ran.status {
        Some(0 | NOT_FOUND) => Ok(()),
        other => Err(failed(
            "delete",
            format!("the program ended with {other:?}"),
        )),
    }
}

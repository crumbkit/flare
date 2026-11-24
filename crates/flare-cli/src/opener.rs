//! Starts the program that opens a file or a URL in the browser. This is
//! one of the command's two process modules (PROC-1); the other is
//! `keychain.rs`.
//!
//! PROC-2's five items:
//!
//! - **Program.** `FLARE_OPENER` when it is set, else `/usr/bin/open` on
//!   macOS and the first `xdg-open` on `PATH` elsewhere. The tests set
//!   `FLARE_OPENER` to a script, so no test starts a browser.
//! - **Arguments.** One: for `flare open`, the opener file's absolute path,
//!   which the server made in flare's state directory; for `flare login`,
//!   the URL of the server's `/cli/login` with the loopback port and the
//!   login's state, which are not secret. No shell reads it.
//! - **Environment.** Inherited, because the program needs the person's
//!   desktop session to reach the browser.
//! - **Streams and deadline.** Standard input is empty, and standard output
//!   and standard error are discarded. The program has 10 seconds, which
//!   `open` and `xdg-open` need only to hand the file to the browser.
//! - **End.** crumbkit-process starts the program in a process group of its
//!   own and, at the deadline, sends the group SIGTERM, waits one second
//!   and kills it. The opener file stays until the server removes it after
//!   sign-in or after the launch code expires.

use crate::codes::Failed;
use crate::env::Snapshot;
use crumbkit_process::{Capture, Escalation, Input, Outcome, RunOptions, Signal, Status};
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

/// How long the program may run.
const DEADLINE: Duration = Duration::from_secs(10);

/// The program that opens a file in the browser.
fn program(env: &Snapshot) -> Option<PathBuf> {
    if let Some(named) = env.get("FLARE_OPENER") {
        return Some(PathBuf::from(named));
    }
    if cfg!(target_os = "macos") {
        return Some(PathBuf::from("/usr/bin/open"));
    }
    std::env::split_paths(&env.search_path())
        .map(|dir| dir.join("xdg-open"))
        .find(|candidate| candidate.is_file())
}

/// Opens `target`, a file or a URL, in the browser.
///
/// # Errors
///
/// Returns [`Failed::OpenerFailed`] when no program is found, it does not
/// start, it exits with a status other than 0 or it runs past its
/// deadline.
pub(crate) fn open(target: impl AsRef<OsStr>, env: &Snapshot) -> Result<(), Failed> {
    let failed = |program: Option<&PathBuf>, problem: String| Failed::OpenerFailed {
        program: program.map(|program| program.display().to_string()),
        problem,
    };
    let program = program(env).ok_or_else(|| failed(None, "no xdg-open on PATH".to_owned()))?;
    #[expect(
        clippy::disallowed_methods,
        reason = "opener.rs is one of the flare command's two process modules (PROC-1)"
    )]
    let mut command = Command::new(&program);
    command.arg(target);
    let deadline = Instant::now()
        .checked_add(DEADLINE)
        .ok_or_else(|| failed(None, "the deadline is too far away".to_owned()))?;
    let options = RunOptions::new()
        .stdin(Input::Null)
        .stdout(Capture::discard())
        .stderr(Capture::discard())
        .deadline(deadline)
        .escalation(Escalation::default().step(Signal::Terminate, Duration::from_secs(1)));
    let finished = crumbkit_process::run(command, &options)
        .map_err(|error| failed(Some(&program), format!("did not start: {error}")))?;
    match finished.outcome() {
        Outcome::Ended(Status::Code(0)) => Ok(()),
        Outcome::TimedOut => Err(failed(
            Some(&program),
            format!("ran past its deadline of {} seconds", DEADLINE.as_secs()),
        )),
        other => Err(failed(Some(&program), format!("ended with {other:?}"))),
    }
}

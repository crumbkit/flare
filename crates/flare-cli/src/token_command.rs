//! Runs the token command that the config's `client` section names, and
//! reads the token it prints. This is one of the command's three process
//! modules (PROC-1); the others are `opener.rs` and `keychain.rs`. The
//! token is never printed, written to a file or put on a command line.
//!
//! PROC-2's five items:
//!
//! - **Program.** The first item of `client.token_command`, by its path or
//!   by a name that `PATH` finds. The person who wrote the config chose it.
//! - **Arguments.** The rest of `client.token_command`, each as a separate
//!   value with no shell.
//! - **Environment.** The command's own environment, plus `FLARE_SERVER`
//!   set to the origin of `client.server`, because a sign-in helper needs
//!   the person's home, `PATH` and session to find its credentials.
//! - **Streams and deadline.** Standard input is empty. Standard output is
//!   kept up to 16 KiB, which crumbkit-process clears from memory when it
//!   drops. Standard error goes to the terminal, so the helper can tell the
//!   person to sign in. The command has `client.token_deadline_seconds`,
//!   20 by default.
//! - **End.** crumbkit-process starts the program in a process group of
//!   its own and reaps it. Past the deadline, or when the output passes
//!   16 KiB, it sends the group SIGTERM, waits one second and kills the
//!   group. A status other than 0 is a failure, and the output is then not
//!   read.

use crate::codes::{Failed, SharingFailed};
use crate::config::{Client, TokenCommand};
use crate::env::Snapshot;
use crumbkit_process::{
    Capture, Escalation, Input, Outcome, RunOptions, Signal, Status, StopReason, Zeroizing,
};
use secrecy::SecretString;
use std::process::Command;
use std::time::{Duration, Instant};

/// The most standard output the command may print.
const MAX_OUTPUT: usize = 16 * 1024;

/// Runs `client`'s token command and returns the token on the first line
/// of its standard output.
///
/// # Errors
///
/// Returns [`SharingFailed::TokenCommandFailed`] when the command does not
/// start or exits with a status other than 0,
/// [`SharingFailed::TokenCommandTimeout`] when it runs past its deadline,
/// and [`SharingFailed::TokenCommandOutput`] when its output is empty,
/// longer than 16 KiB or not one token on its first line.
#[expect(
    clippy::disallowed_methods,
    reason = "PROC-1: token_command.rs is one of the flare command's three process modules"
)]
pub(crate) fn token(client: &Client, env: &Snapshot) -> Result<SecretString, Failed> {
    let TokenCommand {
        program,
        args,
        deadline,
    } = &client.command;
    let failed = |problem: String| -> Failed {
        SharingFailed::TokenCommandFailed {
            program: program.clone(),
            problem,
        }
        .into()
    };
    let output = |problem: &'static str| -> Failed {
        SharingFailed::TokenCommandOutput {
            program: program.clone(),
            problem,
        }
        .into()
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .envs(env.vars())
        .env("FLARE_SERVER", &client.server);
    let until = Instant::now()
        .checked_add(*deadline)
        .ok_or_else(|| failed("has a deadline too far away".to_owned()))?;
    let options = RunOptions::new()
        .stdin(Input::Null)
        .stdout(Capture::whole(MAX_OUTPUT))
        .stderr(Capture::inherit())
        .deadline(until)
        .escalation(Escalation::default().step(Signal::Terminate, Duration::from_secs(1)));
    let finished = crumbkit_process::run(command, &options)
        .map_err(|error| failed(format!("did not start: {error}")))?;
    let over_limit = finished.stdout().over_limit();
    let outcome = finished.outcome();
    let printed = Zeroizing::new(finished.into_output().0.into_bytes());
    match outcome {
        Outcome::TimedOut => {
            return Err(SharingFailed::TokenCommandTimeout {
                program: program.clone(),
                seconds: deadline.as_secs(),
            }
            .into());
        }
        Outcome::Stopped(StopReason::OverLimit) => {
            return Err(output("more than 16 KiB"));
        }
        Outcome::Ended(Status::Code(0)) if over_limit => {
            return Err(output("more than 16 KiB"));
        }
        Outcome::Ended(Status::Code(0)) => {}
        Outcome::Ended(Status::Code(code)) => {
            return Err(failed(format!("exited with status {code}")));
        }
        Outcome::Ended(Status::Signal(signal)) => {
            return Err(failed(format!("was ended by signal {signal}")));
        }
        _ => return Err(failed("was stopped".to_owned())),
    }
    first_line(&printed).map_err(output)
}

/// The token on the first line of `printed`: one or more visible ASCII
/// characters, with the line's end and a carriage return before it left
/// out.
fn first_line(printed: &[u8]) -> Result<SecretString, &'static str> {
    let line = printed
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    if line.is_empty() {
        return Err("nothing on the first line of its output");
    }
    if !line.iter().all(u8::is_ascii_graphic) {
        return Err("a first line that is not one token of visible ASCII characters");
    }
    // The line is visible ASCII, so it is UTF-8.
    let text = std::str::from_utf8(line).map_err(|_| "a first line that is not text")?;
    Ok(SecretString::from(text.to_owned()))
}

#[cfg(test)]
mod tests;

//! Reads the client secret from the macOS Keychain. This is the crate's one
//! process module (PROC-1), and reading the secret is its one operation.
//!
//! The operation, in PROC-2's five items:
//!
//! - **Program.** `/usr/bin/security`, the macOS system tool, found by that
//!   absolute path and never through `PATH`. The operating system supplies
//!   its bytes. Tests with the `testing` feature name another program.
//! - **Arguments.** Four separate values, `find-generic-password -s
//!   <service> -w`, with no shell. The service name is the only value that
//!   varies, and it may not start with `-`, so it cannot be read as an
//!   option. The secret is never on the command line.
//! - **Environment.** Empty. The child inherits no variable.
//! - **Streams and deadline.** Standard input is empty, standard error is
//!   discarded and at most 4 KiB of standard output is captured. The
//!   deadline is 10 seconds from the start.
//! - **End.** crumbkit-process starts the program in a process group of its
//!   own and reaps it. Past the deadline, or when the output passes 4 KiB,
//!   it sends the group SIGTERM, waits one second and kills the group. Any
//!   status other than zero is [`SecretError::Failed`]. The program only
//!   reads the Keychain, so it leaves no partial work behind.

use crate::refusal::SecretError;
use crate::secret::ClientSecretSource;
use crumbkit_process::{Capture, Escalation, Input, Outcome, RunError, RunOptions, Signal, Status};
use secrecy::SecretString;
use secrecy::zeroize::Zeroize as _;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

const SECURITY: &str = "/usr/bin/security";
const MAX_OUTPUT: usize = 4096;
/// How long the program has to end after SIGTERM before it is killed.
const KILL_GRACE: Duration = Duration::from_secs(1);
const DEADLINE: Duration = Duration::from_secs(10);

/// Reads the client secret from the macOS Keychain item of one service,
/// such as `flare-oidc-google`, through `/usr/bin/security`. Where that
/// program does not exist, as on Linux, reading gives
/// [`SecretError::Unavailable`].
#[derive(Debug, Clone)]
pub struct KeychainSecret {
    program: PathBuf,
    service: String,
    deadline: Duration,
}

fn check_service(service: &str) -> Result<(), SecretError> {
    let control = service.chars().any(char::is_control);
    if service.is_empty() || service.len() > 255 || service.starts_with('-') || control {
        return Err(SecretError::Service);
    }
    Ok(())
}

impl KeychainSecret {
    /// Reads the generic password item of `service`.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::Service`] for a service name that is empty,
    /// longer than 255 bytes, starts with `-` or holds a control character.
    pub fn new(service: &str) -> Result<Self, SecretError> {
        check_service(service)?;
        Ok(Self {
            program: PathBuf::from(SECURITY),
            service: service.to_owned(),
            deadline: DEADLINE,
        })
    }

    /// Runs `program` in place of `/usr/bin/security`, with `deadline` in
    /// place of 10 seconds.
    ///
    /// # Errors
    ///
    /// Returns [`SecretError::Service`] as [`KeychainSecret::new`] does.
    #[cfg(feature = "testing")]
    pub fn with_program(
        program: impl Into<PathBuf>,
        service: &str,
        deadline: Duration,
    ) -> Result<Self, SecretError> {
        check_service(service)?;
        Ok(Self {
            program: program.into(),
            service: service.to_owned(),
            deadline,
        })
    }
}

impl ClientSecretSource for KeychainSecret {
    fn client_secret(&self) -> Result<SecretString, SecretError> {
        let mut output = run(self)?;
        if output.last() == Some(&b'\n') {
            output.pop();
        }
        if output.is_empty() {
            return Err(SecretError::Empty);
        }
        match String::from_utf8(output) {
            Ok(text) => Ok(SecretString::from(text)),
            Err(error) => {
                error.into_bytes().zeroize();
                Err(SecretError::NotText)
            }
        }
    }
}

/// Runs the program and returns its whole standard output.
#[expect(
    clippy::disallowed_methods,
    reason = "PROC-1: this is the crate's one process module, and reading the secret is its one operation"
)]
fn run(source: &KeychainSecret) -> Result<Vec<u8>, SecretError> {
    let mut command = Command::new(&source.program);
    command
        .args(["find-generic-password", "-s", &source.service, "-w"])
        .env_clear();
    let deadline = Instant::now()
        .checked_add(source.deadline)
        .ok_or(SecretError::TimedOut)?;
    let options = RunOptions::new()
        .stdin(Input::Null)
        .stdout(Capture::whole(MAX_OUTPUT))
        .stderr(Capture::discard())
        .deadline(deadline)
        .escalation(Escalation::default().step(Signal::Terminate, KILL_GRACE));
    let finished = crumbkit_process::run(command, &options).map_err(|error| match error {
        RunError::Start(_) => SecretError::Unavailable,
        _ => SecretError::Failed,
    })?;
    let over_limit = finished.stdout().over_limit();
    let outcome = finished.outcome();
    let mut output = finished.into_output().0.into_bytes();
    let result = match outcome {
        Outcome::TimedOut => Err(SecretError::TimedOut),
        // A child that exits on its own just after it passes the limit
        // ends before the run stops it, with its output cut short.
        _ if over_limit => Err(SecretError::TooLong),
        Outcome::Ended(Status::Code(0)) => Ok(()),
        _ => Err(SecretError::Failed),
    };
    match result {
        Ok(()) => Ok(output),
        Err(error) => {
            output.zeroize();
            Err(error)
        }
    }
}

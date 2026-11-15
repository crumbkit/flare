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
//! - **End.** This module stops and reaps the child. Past the deadline, or
//!   when the output passes 4 KiB, it kills the child and waits for it. It
//!   reads the exit status after the child closes its output, and any
//!   status other than zero is [`SecretError::Failed`]. The program only
//!   reads the Keychain, so it leaves no partial work behind.
//!
//! The deadline stops only the direct child, not any process the child
//! starts (PROC-3). `/usr/bin/security` starts none.

use crate::refusal::SecretError;
use crate::secret::ClientSecretSource;
use secrecy::SecretString;
use secrecy::zeroize::Zeroize as _;
use std::io::Read as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const SECURITY: &str = "/usr/bin/security";
const MAX_OUTPUT: usize = 4096;
/// One byte more than [`MAX_OUTPUT`], which is how much the reader reads to
/// tell output of exactly 4 KiB from longer output.
const READ_LIMIT: u64 = 4097;
const DEADLINE: Duration = Duration::from_secs(10);

/// Reads the client secret from the macOS Keychain item of one service,
/// such as `hangar-oidc-google`, through `/usr/bin/security`. Where that
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
    let mut child = Command::new(&source.program)
        .args(["find-generic-password", "-s", &source.service, "-w"])
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| SecretError::Unavailable)?;
    let Some(stdout) = child.stdout.take() else {
        stop(&mut child);
        return Err(SecretError::Unavailable);
    };
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut output = Vec::new();
        let read = stdout.take(READ_LIMIT).read_to_end(&mut output);
        let _ = sender.send(read.map(|_| output));
    });
    match receiver.recv_timeout(source.deadline) {
        Err(_) => {
            stop(&mut child);
            Err(SecretError::TimedOut)
        }
        Ok(Err(_)) => {
            stop(&mut child);
            Err(SecretError::Failed)
        }
        Ok(Ok(mut output)) if output.len() > MAX_OUTPUT => {
            output.zeroize();
            stop(&mut child);
            Err(SecretError::TooLong)
        }
        Ok(Ok(mut output)) => {
            // Shortcut: the deadline covers the wait for the output to end,
            // and the wait for the exit after it is unbounded. `security`
            // exits as it closes its output, so the wait ends at once. A
            // program that closed its output and kept running would hold
            // `connect` until it exited; a crate such as wait-timeout would
            // bound this wait if another program is ever run here.
            let status = child.wait().map_err(|_| SecretError::Failed)?;
            if status.success() {
                Ok(output)
            } else {
                output.zeroize();
                Err(SecretError::Failed)
            }
        }
    }
}

#[expect(
    clippy::disallowed_types,
    reason = "PROC-1: the process module stops and reaps its own child"
)]
fn stop(child: &mut std::process::Child) {
    // Both calls fail only when the child has already exited and been
    // reaped, which leaves nothing to stop.
    let _ = child.kill();
    let _ = child.wait();
}

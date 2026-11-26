//! The `client` section: the server that the share commands call, and the
//! command that gives them a token for it. When the section is present,
//! the share commands run the command for every call and read neither the
//! saved login nor the Keychain. It holds no secret.

use serde::Deserialize;
use std::time::Duration;

/// How long the token command may run when the section names no deadline.
const DEFAULT_DEADLINE: u64 = 20;
/// The shortest and the longest deadline the section may name, in seconds.
const DEADLINES: std::ops::RangeInclusive<u64> = 1..=120;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClientSection {
    server: String,
    token_command: Vec<String>,
    token_deadline_seconds: Option<u64>,
}

/// The server and the command that gives its token.
#[derive(Debug, Clone)]
pub(crate) struct Client {
    /// The server's origin, such as `https://flare.example.com`.
    pub(crate) server: String,
    /// The token command.
    pub(crate) command: TokenCommand,
}

/// A program and its arguments, run without a shell, that prints a token
/// for the server on its first line.
#[derive(Debug, Clone)]
pub(crate) struct TokenCommand {
    /// The program, by its path or a name that `PATH` finds.
    pub(crate) program: String,
    /// Its arguments.
    pub(crate) args: Vec<String>,
    /// How long it may run before it is stopped.
    pub(crate) deadline: Duration,
}

/// The client settings from the section.
///
/// # Errors
///
/// Returns the problem, naming the key.
pub(super) fn client(section: ClientSection) -> Result<Client, String> {
    let server =
        crate::login::origin(&section.server).map_err(|error| format!("client.server: {error}"))?;
    let mut parts = section.token_command.into_iter();
    let program = parts.next().filter(|program| !program.is_empty()).ok_or_else(|| {
        "client.token_command must be a list whose first item names the program, such as [\"/usr/local/bin/sso-token\", \"--audience\", \"flare\"]".to_owned()
    })?;
    let seconds = section.token_deadline_seconds.unwrap_or(DEFAULT_DEADLINE);
    if !DEADLINES.contains(&seconds) {
        return Err("client.token_deadline_seconds must be from 1 to 120".to_owned());
    }
    Ok(Client {
        server,
        command: TokenCommand {
            program,
            args: parts.collect(),
            deadline: Duration::from_secs(seconds),
        },
    })
}

#[cfg(test)]
mod tests;

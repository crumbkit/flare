//! The saved login: `<state home>/flare/login.json`, which names the server,
//! the token's id and label, and the owner. The token itself is only in the
//! Keychain. crumbkit-durable replaces the file whole with mode 0600 (DUR-1),
//! so a reader sees the old login or the new one.

use crate::board::state_home;
use crate::codes::{Failed, SharingFailed};
use crate::env::Snapshot;
use crumbkit_durable::{Mode, write_atomically};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The largest login file flare reads, in bytes.
const MAX_BYTES: u64 = 4096;

/// What `flare login` saves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Saved {
    /// The server's origin.
    pub(crate) server: String,
    /// The token's id, which `flare logout` revokes.
    pub(crate) token_id: String,
    /// The token's label, which the next login reuses so the server
    /// revokes this token then.
    pub(crate) label: String,
    /// The address the token signs in as.
    pub(crate) owner: String,
}

fn path(env: &Snapshot) -> Result<PathBuf, Failed> {
    Ok(state_home(env)?.join("flare").join("login.json"))
}

fn unusable(problem: impl std::fmt::Display) -> Failed {
    Failed::StateUnusable {
        problem: format!("the saved login could not be used: {problem}"),
    }
}

/// The saved login, or `None` when there is none.
///
/// # Errors
///
/// Returns [`Failed::StateUnusable`] for a file that cannot be read or is
/// not a saved login.
pub(crate) fn load(env: &Snapshot) -> Result<Option<Saved>, Failed> {
    use std::io::Read as _;
    let path = path(env)?;
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(unusable(error)),
    };
    let mut text = String::new();
    file.take(MAX_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(unusable)?;
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        return Err(unusable(format!("it is longer than {MAX_BYTES} bytes")));
    }
    serde_json::from_str(&text).map(Some).map_err(unusable)
}

/// The saved login, or [`SharingFailed::LoginNeeded`].
///
/// # Errors
///
/// Returns [`SharingFailed::LoginNeeded`] and the failures of [`load`].
pub(crate) fn required(env: &Snapshot) -> Result<Saved, Failed> {
    load(env)?.ok_or_else(|| SharingFailed::LoginNeeded { server: None }.into())
}

/// Saves `saved`, replacing any earlier login.
///
/// # Errors
///
/// Returns [`Failed::StateUnusable`].
pub(crate) fn store(env: &Snapshot, saved: &Saved) -> Result<(), Failed> {
    let path = path(env)?;
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).map_err(unusable)?;
    }
    let bytes = serde_json::to_vec_pretty(saved).map_err(unusable)?;
    write_atomically(&path, &bytes, Mode::Private).map_err(unusable)
}

/// Removes the saved login. A missing file counts as removed.
///
/// # Errors
///
/// Returns [`Failed::StateUnusable`].
pub(crate) fn forget(env: &Snapshot) -> Result<(), Failed> {
    match std::fs::remove_file(path(env)?) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(unusable(error)),
    }
}

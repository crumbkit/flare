//! The saved login: `<state home>/flare/login.json`, which names the server,
//! the token's id and label, and the owner. The token itself is only in the
//! Keychain. crumbkit-durable replaces the file whole with mode 0600 (DUR-1),
//! so a reader sees the old login or the new one.
//!
//! `<state home>/flare/labels.json` maps each server's origin to the label
//! of the last login to it. `flare login` writes it before the exchange, so
//! a login whose token never reaches the Keychain still gives its label to
//! the next login, and the server revokes the lost token then. The file
//! keeps one entry per server ever logged in to and never drops one; it
//! holds at most 64 KiB, and a login that would pass that fails with
//! `flare::state::unusable`, which deleting the file clears.

use crate::board::state_home;
use crate::codes::{Failed, SharingFailed};
use crate::env::Snapshot;
use crumbkit_durable::{Mode, write_atomically};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The largest login file flare reads, in bytes.
const MAX_BYTES: u64 = 4096;

/// The largest labels file flare reads or writes, in bytes.
const MAX_LABEL_BYTES: u64 = 64 * 1024;

/// What `flare login` saves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Saved {
    /// The server's origin.
    pub(crate) server: String,
    /// The token's id, which `flare logout` revokes.
    pub(crate) token_id: String,
    /// The token's label.
    pub(crate) label: String,
    /// The address the token signs in as.
    pub(crate) owner: String,
}

fn path(env: &Snapshot) -> Result<PathBuf, Failed> {
    Ok(state_home(env)?.join("flare").join("login.json"))
}

fn labels_path(env: &Snapshot) -> Result<PathBuf, Failed> {
    Ok(state_home(env)?.join("flare").join("labels.json"))
}

/// The text of the file at `path`, at most `limit` bytes, or `None` when
/// there is no file.
fn read_capped(path: &Path, limit: u64) -> Result<Option<String>, Failed> {
    use std::io::Read as _;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(unusable(error)),
    };
    let mut text = String::new();
    file.take(limit + 1)
        .read_to_string(&mut text)
        .map_err(unusable)?;
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > limit {
        return Err(unusable(format!(
            "{} is longer than {limit} bytes",
            path.display()
        )));
    }
    Ok(Some(text))
}

/// Replaces the file at `path` with `bytes`, mode 0600.
fn write(path: &Path, bytes: &[u8]) -> Result<(), Failed> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).map_err(unusable)?;
    }
    write_atomically(path, bytes, Mode::Private).map_err(unusable)
}

fn labels(env: &Snapshot) -> Result<BTreeMap<String, String>, Failed> {
    match read_capped(&labels_path(env)?, MAX_LABEL_BYTES)? {
        Some(text) => serde_json::from_str(&text).map_err(unusable),
        None => Ok(BTreeMap::new()),
    }
}

/// The label of the last login to `server`, or `None`.
///
/// # Errors
///
/// Returns [`Failed::StateUnusable`] for a labels file that cannot be read.
pub(crate) fn label(env: &Snapshot, server: &str) -> Result<Option<String>, Failed> {
    Ok(labels(env)?.remove(server))
}

/// Records `label` as the label of the last login to `server`.
///
/// # Errors
///
/// Returns [`Failed::StateUnusable`], also when the file would pass 64 KiB.
pub(crate) fn remember_label(env: &Snapshot, server: &str, label: &str) -> Result<(), Failed> {
    let mut labels = labels(env)?;
    if labels.get(server).map(String::as_str) == Some(label) {
        return Ok(());
    }
    labels.insert(server.to_owned(), label.to_owned());
    let bytes = serde_json::to_vec_pretty(&labels).map_err(unusable)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_LABEL_BYTES {
        return Err(unusable(format!(
            "the labels file would pass {MAX_LABEL_BYTES} bytes"
        )));
    }
    write(&labels_path(env)?, &bytes)
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
    match read_capped(&path(env)?, MAX_BYTES)? {
        Some(text) => serde_json::from_str(&text).map(Some).map_err(unusable),
        None => Ok(None),
    }
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
    let bytes = serde_json::to_vec_pretty(saved).map_err(unusable)?;
    write(&path(env)?, &bytes)
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

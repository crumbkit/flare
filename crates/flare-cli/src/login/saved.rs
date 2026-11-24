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

#[cfg(test)]
mod tests;

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

/// The two files that this module keeps in flare's state directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateFile {
    /// `login.json`, the saved login.
    Login,
    /// `labels.json`, the label of the last login to each server.
    Labels,
}

impl StateFile {
    /// The file's path under the state home that `env` names.
    fn path(self, env: &Snapshot) -> Result<PathBuf, Failed> {
        let name = match self {
            Self::Login => "login.json",
            Self::Labels => "labels.json",
        };
        Ok(state_home(env)?.join("flare").join(name))
    }

    /// The most bytes flare reads from the file or writes to it.
    const fn limit(self) -> u64 {
        match self {
            Self::Login => MAX_BYTES,
            Self::Labels => MAX_LABEL_BYTES,
        }
    }

    /// What a failure calls the file.
    const fn name(self) -> &'static str {
        match self {
            Self::Login => "the saved login",
            Self::Labels => "the login labels file",
        }
    }

    /// [`Failed::StateUnusable`] for this file at `path`, whose message
    /// names the file and starts with its path.
    fn unusable(self, path: &Path, problem: impl std::fmt::Display) -> Failed {
        Failed::StateUnusable {
            path: Some(path.display().to_string()),
            problem: format!(
                "{} {} could not be used: {problem}",
                self.name(),
                path.display()
            ),
        }
    }
}

/// The text of `file` at `path`, at most its limit in bytes, or `None`
/// when there is no file.
fn read_capped(file: StateFile, path: &Path) -> Result<Option<String>, Failed> {
    use std::io::Read as _;
    let limit = file.limit();
    let opened = match std::fs::File::open(path) {
        Ok(opened) => opened,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(file.unusable(path, error)),
    };
    let mut text = String::new();
    opened
        .take(limit + 1)
        .read_to_string(&mut text)
        .map_err(|error| file.unusable(path, error))?;
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > limit {
        return Err(file.unusable(path, format!("it is longer than {limit} bytes")));
    }
    Ok(Some(text))
}

/// Replaces `file` at `path` with `bytes`, mode 0600, creating its
/// directory first.
fn write(path: &Path, file: StateFile, bytes: &[u8]) -> Result<(), Failed> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .map_err(|error| directory_uncreatable(directory, error))?;
    }
    write_atomically(path, bytes, Mode::Private).map_err(|error| file.unusable(path, error))
}

/// [`Failed::StateUnusable`] for a state directory that `create_dir_all`
/// could not create.
fn directory_uncreatable(directory: &Path, error: std::io::Error) -> Failed {
    Failed::StateUnusable {
        path: Some(directory.display().to_string()),
        problem: format!(
            "the state directory {} could not be created: {error}",
            directory.display()
        ),
    }
}

fn labels(env: &Snapshot) -> Result<BTreeMap<String, String>, Failed> {
    let file = StateFile::Labels;
    let path = file.path(env)?;
    match read_capped(file, &path)? {
        Some(text) => serde_json::from_str(&text).map_err(|error| file.unusable(&path, error)),
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
    let file = StateFile::Labels;
    let path = file.path(env)?;
    let bytes = serde_json::to_vec_pretty(&labels).map_err(|error| file.unusable(&path, error))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > file.limit() {
        return Err(file.unusable(&path, format!("it would pass {MAX_LABEL_BYTES} bytes")));
    }
    write(&path, file, &bytes)
}

/// The saved login, or `None` when there is none.
///
/// # Errors
///
/// Returns [`Failed::StateUnusable`] for a file that cannot be read or is
/// not a saved login.
pub(crate) fn load(env: &Snapshot) -> Result<Option<Saved>, Failed> {
    let file = StateFile::Login;
    let path = file.path(env)?;
    match read_capped(file, &path)? {
        Some(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|error| file.unusable(&path, error)),
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
    let file = StateFile::Login;
    let path = file.path(env)?;
    let bytes = serde_json::to_vec_pretty(saved).map_err(|error| file.unusable(&path, error))?;
    write(&path, file, &bytes)
}

/// Removes the saved login. A missing file counts as removed.
///
/// # Errors
///
/// Returns [`Failed::StateUnusable`].
pub(crate) fn forget(env: &Snapshot) -> Result<(), Failed> {
    let file = StateFile::Login;
    let path = file.path(env)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(file.unusable(&path, error)),
    }
}

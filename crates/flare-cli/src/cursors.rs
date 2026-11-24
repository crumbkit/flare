//! The activity cursors this computer keeps for `flare activity
//! --since-last`: `<state home>/flare/cursors.json` maps each server's
//! origin to its named cursors and each name to the position last read.
//!
//! A cursor only moves forward. The file holds at most 64 KiB; a save that
//! would pass that fails with `flare::state::unusable`, which deleting the
//! file clears. The server keeps its own cursors under the same names, and
//! a call reads from the larger of the two.

use crate::codes::Failed;
use crate::env::Snapshot;
use crate::login::saved::{StateFile, read_capped, write};
use flare_contract::Cursor;
use std::collections::BTreeMap;

/// The file this module keeps.
const FILE: StateFile = StateFile::Cursors;

/// Each server's named cursors, as cursor text.
type Saved = BTreeMap<String, BTreeMap<String, String>>;

fn load(env: &Snapshot) -> Result<Saved, Failed> {
    let path = FILE.path(env)?;
    match read_capped(FILE, &path)? {
        Some(text) => serde_json::from_str(&text).map_err(|error| FILE.unusable(&path, error)),
        None => Ok(Saved::new()),
    }
}

/// The cursor saved under `name` for `server`, or `None`.
///
/// # Errors
///
/// Returns [`Failed::StateUnusable`] for a file that cannot be read or
/// holds text that is not a cursor.
pub(crate) fn read(env: &Snapshot, server: &str, name: &str) -> Result<Option<Cursor>, Failed> {
    let path = FILE.path(env)?;
    let saved = load(env)?;
    saved
        .get(server)
        .and_then(|names| names.get(name))
        .map(|text| Cursor::try_from(text.as_str()).map_err(|error| FILE.unusable(&path, error)))
        .transpose()
}

/// Saves `cursor` under `name` for `server`, unless a larger one is saved.
///
/// # Errors
///
/// Returns [`Failed::StateUnusable`] for a file that cannot be read or
/// written, and when the file would pass 64 KiB.
pub(crate) fn save(env: &Snapshot, server: &str, name: &str, cursor: Cursor) -> Result<(), Failed> {
    if read(env, server, name)?.is_some_and(|saved| saved >= cursor) {
        return Ok(());
    }
    let mut saved = load(env)?;
    saved
        .entry(server.to_owned())
        .or_default()
        .insert(name.to_owned(), cursor.to_string());
    let path = FILE.path(env)?;
    let bytes = serde_json::to_vec_pretty(&saved).map_err(|error| FILE.unusable(&path, error))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > FILE.limit() {
        return Err(FILE.unusable(&path, format!("it would pass {} bytes", FILE.limit())));
    }
    write(&path, FILE, &bytes)
}

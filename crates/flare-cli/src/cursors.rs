//! The activity cursors this computer keeps for `flare activity
//! --since-last`: `<state home>/flare/cursors.json` maps each server's
//! origin to its named cursors and each name to the position last read.
//!
//! A cursor only moves forward. The file holds at most 64 KiB; a save that
//! would pass that fails with `flare::state::unusable`, which deleting the
//! file clears. The server keeps its own cursors under the same names, and
//! a call reads from the larger of the two.

use crate::board::state_home;
use crate::codes::Failed;
use crate::env::Snapshot;
use crate::login::saved::{read_capped, unusable, write};
use flare_contract::Cursor;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The largest cursors file flare reads or writes, in bytes.
const MAX_BYTES: u64 = 64 * 1024;

/// Each server's named cursors, as cursor text.
type Saved = BTreeMap<String, BTreeMap<String, String>>;

fn path(env: &Snapshot) -> Result<PathBuf, Failed> {
    Ok(state_home(env)?.join("flare").join("cursors.json"))
}

fn load(env: &Snapshot) -> Result<Saved, Failed> {
    match read_capped(&path(env)?, MAX_BYTES)? {
        Some(text) => serde_json::from_str(&text).map_err(unusable),
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
    let saved = load(env)?;
    saved
        .get(server)
        .and_then(|names| names.get(name))
        .map(|text| Cursor::try_from(text.as_str()).map_err(unusable))
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
    let bytes = serde_json::to_vec_pretty(&saved).map_err(unusable)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BYTES {
        return Err(unusable(format!(
            "the cursors file would pass {MAX_BYTES} bytes"
        )));
    }
    write(&path(env)?, &bytes)
}

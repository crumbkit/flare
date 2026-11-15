//! The file system calls of the state directory: the trust checks, the
//! atomic write, new private files, removal and the sweep.

use super::switches::{ReadStep, Switches};
use super::{StateError, Untrusted};
use crate::secret::{is_hex, with_bytes};
use secrecy::SecretString;
use std::fs::{File, Metadata, OpenOptions, Permissions};
use std::io::{self, Read as _, Write as _};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How old a temporary file or an opener must be before the sweep removes
/// it. A launch code lives as long, so an opener this old holds a dead
/// code.
const SWEEP_AGE: Duration = Duration::from_secs(60);

/// The user this process runs as. A new pipe belongs to the user that made
/// it, so its owner is this process's user. The standard library has no
/// safe way to ask for the user id, and the crate forbids unsafe code.
pub(super) fn user() -> io::Result<u32> {
    let (reader, _writer) = io::pipe()?;
    Ok(File::from(std::os::fd::OwnedFd::from(reader))
        .metadata()?
        .uid())
}

fn untrusted(path: &Path, reason: Untrusted) -> StateError {
    StateError::Untrusted {
        path: path.to_owned(),
        reason,
    }
}

fn io_error(path: &Path, source: io::Error) -> StateError {
    StateError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Checks that `dir` is a directory, not a link, owned by `user`, with
/// mode 0700.
pub(super) fn check_dir(dir: &Path, user: u32) -> Result<(), StateError> {
    let metadata = std::fs::symlink_metadata(dir).map_err(|source| io_error(dir, source))?;
    dir_trusted(dir, &metadata, user)
}

fn dir_trusted(dir: &Path, metadata: &Metadata, user: u32) -> Result<(), StateError> {
    if metadata.file_type().is_symlink() {
        return Err(untrusted(dir, Untrusted::Link));
    }
    if !metadata.is_dir() {
        return Err(untrusted(dir, Untrusted::NotDirectory));
    }
    if metadata.uid() != user {
        return Err(untrusted(dir, Untrusted::Owner));
    }
    match metadata.mode() & 0o7777 {
        0o700 => Ok(()),
        mode => Err(untrusted(dir, Untrusted::Mode(mode))),
    }
}

/// Checks a server file's type, owner, mode and links. It accepts 0 links
/// as well as 1: when a newer `write_server` renames over the file during
/// a read, the file the reader found has no link left, and it still holds
/// the whole old record. A second link is what the check refuses, because
/// another path could then change the file.
fn file_trusted(path: &Path, metadata: &Metadata, user: u32) -> Result<(), StateError> {
    if metadata.file_type().is_symlink() {
        return Err(untrusted(path, Untrusted::Link));
    }
    if !metadata.is_file() {
        return Err(untrusted(path, Untrusted::NotRegular));
    }
    if metadata.uid() != user {
        return Err(untrusted(path, Untrusted::Owner));
    }
    match (metadata.mode() & 0o7777, metadata.nlink()) {
        (0o600, 0 | 1) => Ok(()),
        (0o600, links) => Err(untrusted(path, Untrusted::Links(links))),
        (mode, _) => Err(untrusted(path, Untrusted::Mode(mode))),
    }
}

/// What [`open_checked`] found.
pub(super) enum Opened {
    /// The directory or the file is missing.
    Missing,
    /// The file's bytes.
    Read(Vec<u8>),
}

fn missing_is_none<T>(result: io::Result<T>) -> io::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// How many times `open_checked` checks and opens the file before it
/// reports [`Untrusted::Replaced`]. A `write_server` that renames over the
/// file between the check and the open replaces it once; a file that
/// changes on every attempt is refused.
const OPEN_ATTEMPTS: usize = 3;

/// Reads `path` in `dir` after checking both, and checks the open file by
/// device and inode against the `lstat`, which catches a link swapped in
/// between the two. A file replaced in that window is checked and opened
/// again, up to three attempts in all. A file over `max` bytes reads as its
/// first `max + 1` bytes, which no record parses.
pub(super) fn open_checked(
    dir: &Path,
    path: &Path,
    user: u32,
    max: u64,
    switches: &Switches,
) -> Result<Opened, StateError> {
    let mut attempt = 1;
    loop {
        match open_once(dir, path, user, max, switches) {
            Err(StateError::Untrusted {
                reason: Untrusted::Replaced,
                ..
            }) if attempt < OPEN_ATTEMPTS => attempt += 1,
            answer => return answer,
        }
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "the state module owns the server file; this open only reads"
)]
fn open_once(
    dir: &Path,
    path: &Path,
    user: u32,
    max: u64,
    switches: &Switches,
) -> Result<Opened, StateError> {
    let Some(dir_metadata) =
        missing_is_none(std::fs::symlink_metadata(dir)).map_err(|source| io_error(dir, source))?
    else {
        return Ok(Opened::Missing);
    };
    dir_trusted(dir, &dir_metadata, user)?;
    let Some(link) = missing_is_none(std::fs::symlink_metadata(path))
        .map_err(|source| io_error(path, source))?
    else {
        return Ok(Opened::Missing);
    };
    file_trusted(path, &link, user)?;
    switches.at(ReadStep::Checked, path);
    // O_NONBLOCK keeps a FIFO put in place of the file from blocking the
    // open; the checks after the open then refuse it as not a regular file.
    let opening = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path);
    let Some(file) = missing_is_none(opening).map_err(|source| io_error(path, source))? else {
        return Ok(Opened::Missing);
    };
    switches.at(ReadStep::Opened, path);
    let opened = file.metadata().map_err(|source| io_error(path, source))?;
    if opened.dev() != link.dev() || opened.ino() != link.ino() {
        return Err(untrusted(path, Untrusted::Replaced));
    }
    file_trusted(path, &opened, user)?;
    let mut bytes = Vec::new();
    file.take(max.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| io_error(path, source))?;
    Ok(Opened::Read(bytes))
}

/// Creates `path`, which must not exist, with mode 0600, writes `contents`
/// and syncs it. When a step after the create fails, it removes the file
/// it created; a file that was already at `path` is never removed.
#[expect(
    clippy::disallowed_methods,
    reason = "the state module is the crate's atomic writer (DUR-1)"
)]
pub(super) fn write_new(path: &Path, contents: &SecretString) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    let written = file
        // The mode given to open is reduced by the umask, so it is set again.
        .set_permissions(Permissions::from_mode(0o600))
        .and_then(|()| with_bytes(contents, |bytes| file.write_all(bytes)))
        .and_then(|()| file.sync_all());
    written.inspect_err(|_| {
        // A file left half written holds part of a secret. Removing it is
        // tried once, and the sweep removes it otherwise.
        let _ = remove_if_present(path);
    })
}

/// Writes `contents` to `temporary`, renames it over `target` and syncs
/// `dir` (DUR-1). On failure it names the path whose step failed.
#[expect(
    clippy::disallowed_methods,
    reason = "the state module is the crate's atomic writer (DUR-1)"
)]
pub(super) fn replace(
    temporary: &Path,
    target: &Path,
    contents: &SecretString,
    dir: &Path,
) -> Result<(), (PathBuf, io::Error)> {
    write_new(temporary, contents).map_err(|error| (temporary.to_owned(), error))?;
    std::fs::rename(temporary, target).map_err(|error| {
        let _ = remove_if_present(temporary);
        (target.to_owned(), error)
    })?;
    File::open(dir)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| (dir.to_owned(), error))
}

/// Removes `path`; one already gone is fine.
pub(super) fn remove_if_present(path: &Path) -> io::Result<()> {
    missing_is_none(std::fs::remove_file(path)).map(|_| ())
}

/// Whether `name` is `open-<16 hex>.html`.
pub(super) fn is_opener_name(name: &str) -> bool {
    name.strip_prefix("open-")
        .and_then(|rest| rest.strip_suffix(".html"))
        .is_some_and(|hex| is_hex(hex, 16))
}

/// Whether `name` is `server.json.<16 hex>.tmp`.
fn is_temporary_name(name: &str) -> bool {
    name.strip_prefix("server.json.")
        .and_then(|rest| rest.strip_suffix(".tmp"))
        .is_some_and(|hex| is_hex(hex, 16))
}

/// Removes the regular files named as temporary files or openers whose
/// modification time is more than 60 seconds before `now`. Every other
/// file stays. The sweep tidies up; a file it cannot read or remove is
/// left for the next sweep.
pub(super) fn sweep(dir: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let ours = name
            .to_str()
            .is_some_and(|name| is_opener_name(name) || is_temporary_name(name));
        let old = entry
            .metadata()
            .ok()
            .filter(Metadata::is_file)
            .and_then(|metadata| {
                let modified = metadata.modified().ok()?;
                now.duration_since(modified).ok()
            });
        if ours && old.is_some_and(|age| age > SWEEP_AGE) {
            let _ = remove_if_present(&entry.path());
        }
    }
}

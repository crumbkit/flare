//! The state directory of one board: `<state home>/hangar/loopback/<board
//! key>/`, mode 0700. It holds the server file, `server.json` (mode 0600),
//! and the openers, `open-<16 hex>.html` (mode 0600). This module and
//! `state/files.rs` are the crate's only file system code (BASE-5, DUR-1).

mod files;
mod switches;

use crate::record::{ServerRecord, Unreadable};
use crate::secret::{hex, random_hex, sha256};
use files::{Opened, check_dir, open_checked, remove_if_present, sweep, write_new};
use secrecy::SecretString;
use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
pub use switches::ReadStep;
use switches::Switches;

/// The server file's name.
const SERVER_FILE: &str = "server.json";
/// The largest server file the crate reads, in bytes. A version 1 record
/// is under 200 bytes.
const MAX_SERVER_FILE: u64 = 4096;

/// The state home: `xdg_state_home` when it is an absolute path, and
/// `<home>/.local/state` otherwise. The caller passes the values of
/// `XDG_STATE_HOME` and `HOME` from its environment snapshot.
///
/// # Errors
///
/// Returns [`StateError::NoStateHome`] when neither gives an absolute path.
pub fn state_home(
    xdg_state_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Result<PathBuf, StateError> {
    if let Some(xdg) = absolute(xdg_state_home) {
        return Ok(xdg.to_owned());
    }
    absolute(home)
        .map(|home| home.join(".local/state"))
        .ok_or(StateError::NoStateHome)
}

fn absolute(value: Option<&OsStr>) -> Option<&Path> {
    value.map(Path::new).filter(|path| path.is_absolute())
}

/// The state directory of one board.
#[derive(Debug, Clone)]
pub struct StateDir {
    dir: PathBuf,
    switches: Switches,
}

impl StateDir {
    /// The state directory under `state_home` of the board whose canonical
    /// root path is `board_root`. The board key is the first 32 hex digits
    /// of the SHA-256 of that path, so each board has its own directory.
    #[must_use]
    pub fn for_board(state_home: &Path, board_root: &Path) -> Self {
        let digest = hex(&sha256(board_root.as_os_str().as_bytes()));
        let key = digest.get(..32).unwrap_or(&digest);
        Self {
            dir: state_home.join("hangar").join("loopback").join(key),
            switches: Switches::default(),
        }
    }

    /// The directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// Makes the directory with mode 0700, or checks the one that is
    /// there, then removes temporary files and openers more than 60
    /// seconds older than `now`. It never changes the mode of a directory
    /// it finds.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::Untrusted`] when the directory is a link, is
    /// not a directory, belongs to another user or has a mode other than
    /// 0700, and [`StateError::Io`] when it cannot be made.
    pub fn prepare(&self, now: SystemTime) -> Result<(), StateError> {
        if let Some(parent) = self.dir.parent() {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .recursive(true)
                .create(parent)
                .map_err(|source| io_error(parent, source))?;
        }
        match std::fs::DirBuilder::new().mode(0o700).create(&self.dir) {
            Err(source) if source.kind() != io::ErrorKind::AlreadyExists => {
                return Err(io_error(&self.dir, source));
            }
            _ => {}
        }
        let user = self
            .switches
            .user()
            .map_err(|source| io_error(&self.dir, source))?;
        check_dir(&self.dir, user)?;
        sweep(&self.dir, now);
        Ok(())
    }

    /// Writes the server file through crumbkit-durable: a new temporary file
    /// of mode 0600, synced, renamed over `server.json`, then the directory
    /// synced. A reader sees
    /// the whole old file or the whole new one. A bad file in the way is
    /// replaced by the rename and never opened.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::Untrusted`] for an untrusted directory and
    /// [`StateError::Io`] when a step fails. `server.json` is then
    /// unchanged, or wholly replaced if only the directory sync failed.
    pub fn write_server(&self, record: &ServerRecord) -> Result<(), StateError> {
        let user = self
            .switches
            .user()
            .map_err(|source| io_error(&self.dir, source))?;
        check_dir(&self.dir, user)?;
        let target = self.dir.join(SERVER_FILE);
        files::replace(&target, &record.to_json()).map_err(|(path, source)| io_error(&path, source))
    }

    /// Reads the server file. `None` when the directory or the file is
    /// missing. The file is checked with `lstat` before it is opened and by
    /// device and inode after.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::Untrusted`] for a directory or file that fails
    /// the checks, [`StateError::Newer`] for a version above 1,
    /// [`StateError::Malformed`] for a file that is not a record, and
    /// [`StateError::Io`] when a read fails.
    pub fn read_server(&self) -> Result<Option<ServerRecord>, StateError> {
        let user = self
            .switches
            .user()
            .map_err(|source| io_error(&self.dir, source))?;
        let path = self.dir.join(SERVER_FILE);
        let bytes = match open_checked(&self.dir, &path, user, MAX_SERVER_FILE, &self.switches)? {
            Opened::Missing => return Ok(None),
            Opened::Read(bytes) => bytes,
        };
        match ServerRecord::parse(&bytes) {
            Ok(record) => Ok(Some(record)),
            Err(Unreadable::Newer(version)) => Err(StateError::Newer { path, version }),
            Err(Unreadable::Malformed) => Err(StateError::Malformed { path }),
        }
    }

    /// Removes the server file only while it holds `own`'s token, compared
    /// in constant time, so a server that exits leaves the file of a
    /// server that replaced it (DUR-4). A file that names another server,
    /// a malformed one and a missing one are left, and the answer is `Ok`.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::Untrusted`] for an untrusted directory or file,
    /// and [`StateError::Io`] when the read or the removal fails.
    pub fn remove_server(&self, own: &ServerRecord) -> Result<(), StateError> {
        let current = match self.read_server() {
            Ok(Some(current)) => current,
            Ok(None) | Err(StateError::Malformed { .. } | StateError::Newer { .. }) => {
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if !current.same_token(own) {
            return Ok(());
        }
        let path = self.dir.join(SERVER_FILE);
        remove_if_present(&path).map_err(|source| io_error(&path, source))
    }

    /// Writes `page` as a new opener, `open-<16 hex>.html`, with mode 0600,
    /// and returns its path.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::Untrusted`] for an untrusted directory,
    /// [`StateError::Random`] without random bytes and [`StateError::Io`]
    /// when the write fails.
    pub fn write_opener(&self, page: &SecretString) -> Result<PathBuf, StateError> {
        let user = self
            .switches
            .user()
            .map_err(|source| io_error(&self.dir, source))?;
        check_dir(&self.dir, user)?;
        let suffix = random_hex(8).map_err(|_| StateError::Random)?;
        let opener = self.dir.join(format!("open-{suffix}.html"));
        write_new(&opener, page).map_err(|source| io_error(&opener, source))?;
        Ok(opener)
    }

    /// Removes an opener of this directory. One that is already gone is
    /// fine.
    ///
    /// # Errors
    ///
    /// Returns [`StateError::NotOpener`] for a path that is not
    /// `open-<16 hex>.html` in this directory, and [`StateError::Io`] when
    /// the removal fails.
    pub fn remove_opener(&self, opener: &Path) -> Result<(), StateError> {
        let named = opener
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(files::is_opener_name);
        if !named || opener.parent() != Some(self.dir.as_path()) {
            return Err(StateError::NotOpener {
                path: opener.to_owned(),
            });
        }
        remove_if_present(opener).map_err(|source| io_error(opener, source))
    }
}

fn io_error(path: &Path, source: io::Error) -> StateError {
    StateError::Io {
        path: path.to_owned(),
        source,
    }
}

/// Why the state directory or a file in it could not be used.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StateError {
    /// Neither `XDG_STATE_HOME` nor `HOME` is an absolute path.
    #[error(
        "neither XDG_STATE_HOME nor HOME is an absolute path, so hangar has no state directory"
    )]
    NoStateHome,
    /// A directory or file is not one the crate trusts with the token. The
    /// crate never repairs it.
    #[error("{} is not safe to use: {reason}; remove it and run hangar again", path.display())]
    Untrusted {
        /// The directory or file.
        path: PathBuf,
        /// What is wrong with it.
        reason: Untrusted,
    },
    /// The server file is not a version 1 record.
    #[error("{} is not a hangar server file; the next hangar serve replaces it", path.display())]
    Malformed {
        /// The server file.
        path: PathBuf,
    },
    /// The server file has a version newer than this crate knows (DUR-3).
    #[error("{} has version {version}, which this hangar cannot read; upgrade hangar", path.display())]
    Newer {
        /// The server file.
        path: PathBuf,
        /// The file's version.
        version: u64,
    },
    /// The path given to `remove_opener` is not an opener of this
    /// directory.
    #[error("{} is not an opener of this state directory", path.display())]
    NotOpener {
        /// The path given.
        path: PathBuf,
    },
    /// The operating system gave no random bytes for a file name.
    #[error("the operating system gave no random bytes")]
    Random,
    /// A file system call failed.
    #[error("{}: {source}", path.display())]
    Io {
        /// The path of the call.
        path: PathBuf,
        /// The error.
        source: io::Error,
    },
}

/// What is wrong with an untrusted directory or file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Untrusted {
    /// It is a symbolic link.
    Link,
    /// It should be a directory and is not.
    NotDirectory,
    /// It should be a regular file and is not.
    NotRegular,
    /// Another user owns it.
    Owner,
    /// Its permission bits are these, and not 0700 for the directory or
    /// 0600 for the file.
    Mode(u32),
    /// It has this many hard links, and not one.
    Links(u64),
    /// It changed between the check and the open.
    Replaced,
}

impl fmt::Display for Untrusted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Link => formatter.write_str("it is a symbolic link"),
            Self::NotDirectory => formatter.write_str("it is not a directory"),
            Self::NotRegular => formatter.write_str("it is not a regular file"),
            Self::Owner => formatter.write_str("another user owns it"),
            Self::Mode(mode) => write!(formatter, "it has mode {mode:04o}"),
            Self::Links(count) => write!(formatter, "it has {count} hard links"),
            Self::Replaced => formatter.write_str("it was replaced while it was opened"),
        }
    }
}

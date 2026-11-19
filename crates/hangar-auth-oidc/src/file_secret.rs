//! Reads the client secret from a file, such as a container's secret mount,
//! on a host that has no macOS Keychain.
//!
//! The file is opened with `O_RDONLY | O_NONBLOCK | O_CLOEXEC`. The open
//! follows symbolic links, as a Kubernetes secret mount needs
//! (`<key> -> ..data/<key>`), and a FIFO in the file's place cannot block
//! it. Every check then runs on the open descriptor through `fstat`, so the
//! file checked is the file read: a regular file, owned by the user this
//! process runs as or by root, not writable by its group or by others, and
//! 1 to 4,096 bytes long. One trailing newline is trimmed.
//!
//! The checks do not refuse a file whose path someone else can swap,
//! because the secret only ever goes over TLS to the issuer's token
//! endpoint, so a swapped file gains them nothing. A world-readable file is
//! accepted, because container runtimes mount secrets 0444 or 0644 by
//! default.

use crate::refusal::SecretError;
use crate::secret::ClientSecretSource;
use secrecy::SecretString;
use secrecy::zeroize::Zeroize as _;
use std::fs::{File, OpenOptions};
use std::io::{self, Read as _};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};

/// The longest secret the file may hold, in bytes.
const MAX_BYTES: u64 = 4096;

/// Reads the client secret from one file. `connect` reads it once. Its
/// `Debug` output names the path and never the secret.
#[derive(Debug, Clone)]
pub struct FileSecret {
    path: PathBuf,
}

impl FileSecret {
    /// Reads the secret from `path`.
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// The file's path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// What kind of file the descriptor names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A regular file.
    File,
    /// A directory, a FIFO, a device or a socket.
    Other,
}

/// The rule on the open file: a regular file, owned by `user` or root,
/// and writable by its owner alone.
pub(crate) fn check(kind: Kind, owner: u32, mode: u32, user: u32) -> Result<(), SecretError> {
    if kind != Kind::File {
        return Err(SecretError::NotAFile);
    }
    if owner != user && owner != 0 {
        return Err(SecretError::FileOwner);
    }
    if mode & 0o022 != 0 {
        return Err(SecretError::FileWritable);
    }
    Ok(())
}

/// The user this process runs as. A new pipe belongs to the user that made
/// it, so its owner is this process's user. The standard library has no
/// safe way to ask for the user id, and the crate forbids unsafe code. This
/// copies hangar-auth-loopback's `state/files.rs`, whose function is
/// private to that crate.
fn user() -> io::Result<u32> {
    let (reader, _writer) = io::pipe()?;
    Ok(File::from(std::os::fd::OwnedFd::from(reader))
        .metadata()?
        .uid())
}

#[expect(
    clippy::disallowed_methods,
    reason = "file_secret.rs only opens the secret file to read it; it writes no file"
)]
fn open(path: &Path) -> Result<File, SecretError> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| SecretError::FileUnreadable)
}

impl ClientSecretSource for FileSecret {
    fn client_secret(&self) -> Result<SecretString, SecretError> {
        let file = open(&self.path)?;
        let metadata = file.metadata().map_err(|_| SecretError::FileUnreadable)?;
        let kind = if metadata.is_file() {
            Kind::File
        } else {
            Kind::Other
        };
        let user = user().map_err(|_| SecretError::FileUnreadable)?;
        check(kind, metadata.uid(), metadata.mode(), user)?;
        let mut bytes = Vec::new();
        if file.take(MAX_BYTES + 1).read_to_end(&mut bytes).is_err() {
            bytes.zeroize();
            return Err(SecretError::FileUnreadable);
        }
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_BYTES {
            bytes.zeroize();
            return Err(SecretError::TooLong);
        }
        if bytes.last() == Some(&b'\n') {
            bytes.pop();
        }
        if bytes.is_empty() {
            return Err(SecretError::Empty);
        }
        match String::from_utf8(bytes) {
            Ok(text) => Ok(SecretString::from(text)),
            Err(error) => {
                error.into_bytes().zeroize();
                Err(SecretError::NotText)
            }
        }
    }
}

#[cfg(test)]
mod tests;

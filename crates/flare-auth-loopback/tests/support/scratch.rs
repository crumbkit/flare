//! A temporary directory for the file tests, and the fixture writes they
//! need.
//!
//! This does less than the tempfile crate on purpose, so the crate adds no
//! dependency: it makes one directory under `/tmp` with a name from the
//! process id and a counter, and removes it on drop. It does not retry a
//! name that exists. It uses `/tmp` and a short name, and not the system's
//! temporary directory, because a Unix socket's path in the state directory
//! must stay under the 104 bytes that macOS allows.

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A directory that is removed with everything in it when dropped.
#[derive(Debug)]
pub struct Scratch(PathBuf);

impl Scratch {
    /// Makes a new, empty directory.
    pub fn new() -> Self {
        let name = format!(
            "hl-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = Path::new("/tmp").join(name);
        std::fs::create_dir(&path).unwrap();
        Self(std::fs::canonicalize(path).unwrap())
    }

    /// The directory.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // A test may leave a directory without write permission, which
        // would stop remove_dir_all.
        restore(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn restore(path: &Path) {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return;
    };
    if metadata.is_dir() {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                restore(&entry.path());
            }
        }
    }
}

/// Writes `bytes` to `path`, replacing what is there.
#[expect(
    clippy::disallowed_methods,
    reason = "a test writes its fixture files directly, outside the state module"
)]
pub fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}

/// Sets the permission bits of `path`.
pub fn chmod(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// The permission bits of `path`.
pub fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::MetadataExt as _;
    std::fs::symlink_metadata(path).unwrap().mode() & 0o7777
}

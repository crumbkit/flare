//! The environment snapshot: the variables and the working directory,
//! read once when the command starts (BASE-6). Every other module reads
//! them from here.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

/// The environment as the command found it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Snapshot {
    vars: HashMap<OsString, OsString>,
    current_dir: Option<PathBuf>,
}

impl Snapshot {
    /// Reads the process's environment and working directory.
    pub(crate) fn capture() -> Self {
        #[expect(
            clippy::disallowed_methods,
            reason = "the entry point captures the environment once (BASE-6)"
        )]
        let vars = std::env::vars_os().collect();
        #[expect(
            clippy::disallowed_methods,
            reason = "the entry point captures the working directory once (BASE-6)"
        )]
        let current_dir = std::env::current_dir().ok();
        Self { vars, current_dir }
    }

    /// The value of `name`, when it is set and not empty.
    pub(crate) fn get(&self, name: &str) -> Option<&OsStr> {
        self.vars
            .get(OsStr::new(name))
            .map(OsString::as_os_str)
            .filter(|value| !value.is_empty())
    }

    /// The working directory, when the process could read it.
    pub(crate) fn current_dir(&self) -> Option<&PathBuf> {
        self.current_dir.as_ref()
    }

    /// `PATH`, or an empty value.
    pub(crate) fn search_path(&self) -> OsString {
        self.get("PATH").map(OsStr::to_owned).unwrap_or_default()
    }

    /// The name of the person who runs the command, from `USER`, for the
    /// local principal `local:<name>`. A missing name gives `owner`.
    pub(crate) fn user(&self) -> String {
        self.get("USER")
            .and_then(OsStr::to_str)
            .filter(|name| name.bytes().all(|byte| byte.is_ascii_graphic()))
            .unwrap_or("owner")
            .to_owned()
    }
}

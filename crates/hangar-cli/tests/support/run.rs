//! Runs the built `hangar` binary in an environment of the test's own.
//!
//! Each [`Home`] is a scratch directory with its own `HOME`, state and
//! config directories, so no test reads or writes the developer's hangar
//! state, usage log or config. The board program comes from
//! `HANGAR_TEST_BOARD`, as hangar-board's tests take it.

use crate::loopback_scratch::Scratch;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// The built binary.
pub const HANGAR: &str = env!("CARGO_BIN_EXE_hangar");

/// A scratch `HOME` with a state directory and a config directory.
#[derive(Debug)]
pub struct Home {
    scratch: Scratch,
}

impl Home {
    /// A new, empty home.
    pub fn new() -> Self {
        let scratch = Scratch::new();
        for directory in ["home", "state", "config"] {
            std::fs::create_dir(scratch.path().join(directory)).unwrap();
        }
        Self { scratch }
    }

    /// The scratch directory, which holds `home`, `state` and `config`.
    pub fn path(&self) -> &Path {
        self.scratch.path()
    }

    /// `XDG_STATE_HOME`.
    pub fn state(&self) -> PathBuf {
        self.path().join("state")
    }

    /// `XDG_CONFIG_HOME`.
    pub fn config(&self) -> PathBuf {
        self.path().join("config")
    }

    /// Writes `text` to `config/hangar/config.json`, where `hangar serve`
    /// looks without `--config`.
    pub fn write_config(&self, text: &str) -> PathBuf {
        let directory = self.config().join("hangar");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.json");
        write(&path, text);
        path
    }

    /// Writes an executable shell script named `name` in the home and
    /// returns its path.
    pub fn script(&self, name: &str, body: &str) -> PathBuf {
        let path = self.path().join(name);
        write(&path, &format!("#!/bin/sh\n{body}"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// The binary with `args`, a cleared environment and this home's
    /// directories. The opener is a script that records the file it is
    /// given, so no test starts a browser.
    pub fn command(&self, args: &[&str]) -> Command {
        #[expect(clippy::disallowed_methods, reason = "the tests run the built binary")]
        let mut command = Command::new(HANGAR);
        command
            .args(args)
            .env_clear()
            .env("HOME", self.path().join("home"))
            .env("XDG_STATE_HOME", self.state())
            .env("XDG_CONFIG_HOME", self.config())
            .env("PATH", search_path())
            .env("USER", "tester")
            .env("HANGAR_OPENER", self.opener())
            .stdin(Stdio::null());
        command
    }

    /// Runs the binary with `args` to its end.
    pub fn run(&self, args: &[&str]) -> Ran {
        #[expect(clippy::disallowed_methods, reason = "the tests run the built binary")]
        let output = self.command(args).output().unwrap();
        Ran::from(output)
    }

    /// The opener script, which appends each path it is given to
    /// `opened.txt` in the home.
    pub fn opener(&self) -> PathBuf {
        let path = self.path().join("opener");
        if !path.exists() {
            let log = self.path().join("opened.txt");
            self.script(
                "opener",
                &format!("printf '%s\\n' \"$1\" >> '{}'\n", log.display()),
            );
        }
        path
    }

    /// The paths the opener script was given, one per run.
    pub fn opened(&self) -> Vec<PathBuf> {
        std::fs::read_to_string(self.path().join("opened.txt"))
            .unwrap_or_default()
            .lines()
            .map(PathBuf::from)
            .collect()
    }

    /// The lines of hangar's usage log.
    pub fn usage_lines(&self) -> usize {
        std::fs::read_to_string(self.state().join("hangar").join("usage.jsonl"))
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }
}

/// The `PATH` the binary gets, which must find `git` for the board.
pub fn search_path() -> std::ffi::OsString {
    #[expect(
        clippy::disallowed_methods,
        reason = "a test passes its own PATH on to the binary"
    )]
    std::env::var_os("PATH").unwrap_or_default()
}

/// Writes a fixture file.
pub fn write(path: &Path, text: &str) {
    #[expect(
        clippy::disallowed_methods,
        reason = "a test writes its fixture files directly"
    )]
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(text.as_bytes()).unwrap();
}

/// What one run printed and how it ended.
#[derive(Debug)]
pub struct Ran {
    /// The exit code, or `None` when a signal ended the run.
    pub code: Option<i32>,
    /// Standard output.
    pub stdout: String,
    /// Standard error.
    pub stderr: String,
}

impl From<Output> for Ran {
    fn from(output: Output) -> Self {
        Self {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }
}

impl Ran {
    /// Both streams, standard output first.
    pub fn all(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    /// Asserts that the run failed with the failure code `code`, such as
    /// `hangar::board::too_old`, and exit code 1.
    pub fn assert_failed(&self, code: &str) {
        assert_eq!(self.code, Some(1), "{code}: {}", self.all());
        assert!(self.all().contains(code), "{code}: {}", self.all());
    }

    /// Asserts that the run succeeded.
    pub fn assert_ok(&self) {
        assert_eq!(self.code, Some(0), "{}", self.all());
    }
}

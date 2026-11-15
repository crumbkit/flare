//! A scratch board for the tests, and the commands that set one up.
//!
//! Each scratch board is a new Git repository under the system's temporary
//! directory with `.board/board.json`, the schemas the tests need and the
//! records each test seeds. The board program comes from
//! `HANGAR_TEST_BOARD`, or from `PATH` when that variable is not set. Every
//! command runs with `HOME` and the XDG directories inside the scratch
//! directory and with `GIT_CONFIG_GLOBAL=/dev/null`, so no test reads or
//! writes the developer's own boards.
//!
//! This does less than the tempfile crate on purpose, so the crate adds no
//! dependency: it makes one directory with a name from the process id and a
//! counter, and removes it on drop. It does not retry a name that exists.

use hangar_board::{BoardConfig, BoardStore, Committer, Limits};
use std::ffi::OsString;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A schema that accepts a task whose `title`, when present, is a string.
pub const TASK_SCHEMA: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"$type":{"const":"tasks/task/v2"},"title":{"type":"string"}}}"#;
/// A schema that accepts any note.
pub const NOTE_SCHEMA: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"$type":{"const":"notes/note/v1"}}}"#;

/// The board program the tests run.
pub fn board_program() -> PathBuf {
    #[expect(
        clippy::disallowed_methods,
        reason = "a test reads its own settings from the environment"
    )]
    if let Some(program) = std::env::var_os("HANGAR_TEST_BOARD") {
        return PathBuf::from(program);
    }
    #[expect(
        clippy::disallowed_methods,
        reason = "a test reads its own settings from the environment"
    )]
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|directory| directory.join("board"))
        .find(|candidate| candidate.is_file())
        .expect(
            "the hangar-board tests need the board program: set HANGAR_TEST_BOARD to its \
             absolute path, or put a board built from crumbkit/board e4ee3333 or later on PATH",
        )
}

/// The `PATH` that the children get, which must hold `git`.
pub fn search_path() -> OsString {
    #[expect(
        clippy::disallowed_methods,
        reason = "a test reads its own settings from the environment"
    )]
    std::env::var_os("PATH").unwrap_or_default()
}

/// A board in a directory that is removed with everything in it on drop.
#[derive(Debug)]
pub struct ScratchBoard {
    directory: PathBuf,
    program: PathBuf,
}

impl ScratchBoard {
    /// A new board holding the task and note schemas, with one commit.
    pub fn new() -> Self {
        let board = Self::empty();
        board.batch(&[
            format!(r#"{{"path":"/.board/types/tasks/task/v2.schema","record":{TASK_SCHEMA}}}"#),
            format!(r#"{{"path":"/.board/types/notes/note/v1.schema","record":{NOTE_SCHEMA}}}"#),
        ]);
        board
    }

    /// A new board with `.board/board.json` and no commit.
    pub fn empty() -> Self {
        let name = format!(
            "hb-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let directory = std::env::temp_dir().join(name);
        std::fs::create_dir_all(directory.join("board")).unwrap();
        std::fs::create_dir_all(directory.join("state")).unwrap();
        let directory = std::fs::canonicalize(directory).unwrap();
        let board = Self {
            directory,
            program: board_program(),
        };
        board.git(&["init", "-q", "-b", "main"]);
        let init = board.board(&["init", "--json"], "");
        assert!(init.status.success(), "board init failed: {init:?}");
        board
    }

    /// The board's working tree.
    pub fn root(&self) -> PathBuf {
        self.directory.join("board")
    }

    /// The state directory that the store's children use as `HOME`.
    pub fn state(&self) -> PathBuf {
        self.directory.join("state")
    }

    /// A directory for a test's own files, beside the board.
    pub fn scratch(&self) -> PathBuf {
        self.directory.clone()
    }

    /// The configuration of a store on this board with `limits`.
    pub fn config_with(&self, program: &Path, limits: Limits) -> BoardConfig {
        BoardConfig::new(
            self.root(),
            program.to_path_buf(),
            self.state(),
            search_path(),
            Committer::new("hangar tests", "hangar-tests@example.invalid").unwrap(),
        )
        .with_limits(limits)
    }

    /// A store on this board with the default limits.
    pub fn store(&self) -> BoardStore {
        BoardStore::open(self.config_with(&self.program, Limits::default())).unwrap()
    }

    /// A store on this board that runs `program` in place of board.
    pub fn store_with(&self, program: &Path, limits: Limits) -> BoardStore {
        BoardStore::open(self.config_with(program, limits)).unwrap()
    }

    /// The real board program.
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Runs the real board in this board with `stdin`.
    pub fn board(&self, args: &[&str], stdin: &str) -> Output {
        self.run(&self.program, args, stdin)
    }

    /// Runs git in this board and returns its standard output.
    pub fn git(&self, args: &[&str]) -> String {
        let output = self.run(Path::new("git"), args, "");
        assert!(output.status.success(), "git {args:?} failed: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    }

    /// Writes the JSON Lines `lines` with one `board put --batch`.
    pub fn batch(&self, lines: &[String]) {
        let mut input = lines.join("\n");
        input.push('\n');
        let output = self.board(&["put", "--batch", "--json"], &input);
        assert!(output.status.success(), "the seed batch failed: {output:?}");
    }

    /// Seeds the JSON record `record` at `path`.
    pub fn seed_record(&self, path: &str, record: &str) {
        let record: serde_json::Value = serde_json::from_str(record).unwrap();
        let line = serde_json::json!({"path": path, "expect": "none", "record": record});
        self.batch(&[line.to_string()]);
    }

    /// Seeds the text file `text` at `path`, its whole name.
    pub fn seed_text(&self, path: &str, text: &str) {
        let line = serde_json::json!({"path": path, "expect": "none", "text": text});
        self.batch(&[line.to_string()]);
    }

    /// The number of commits on HEAD.
    pub fn commits(&self) -> usize {
        self.git(&["rev-list", "--count", "HEAD"])
            .trim()
            .parse()
            .unwrap()
    }

    /// The bytes of the file at `relative` in the working tree.
    pub fn file(&self, relative: &str) -> String {
        std::fs::read_to_string(self.root().join(relative)).unwrap()
    }

    /// Writes an executable shell script named `name` beside the board and
    /// returns its path.
    pub fn script(&self, name: &str, body: &str) -> PathBuf {
        let path = self.directory.join(name);
        #[expect(
            clippy::disallowed_methods,
            reason = "a test writes its own fixture script"
        )]
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn run(&self, program: &Path, args: &[&str], stdin: &str) -> Output {
        let state = self.state();
        #[expect(
            clippy::disallowed_methods,
            reason = "a test sets up its scratch board with git and board"
        )]
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(self.root())
            .env_clear()
            .env("PATH", search_path())
            .env("HOME", &state)
            .env("XDG_CONFIG_HOME", state.join("config"))
            .env("XDG_DATA_HOME", state.join("data"))
            .env("XDG_STATE_HOME", state.join("state"))
            .env("XDG_CACHE_HOME", state.join("cache"))
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "seed")
            .env("GIT_AUTHOR_EMAIL", "seed@example.invalid")
            .env("GIT_COMMITTER_NAME", "seed")
            .env("GIT_COMMITTER_EMAIL", "seed@example.invalid")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[expect(
            clippy::disallowed_methods,
            reason = "a test sets up its scratch board with git and board"
        )]
        let mut child = command.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

impl Drop for ScratchBoard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

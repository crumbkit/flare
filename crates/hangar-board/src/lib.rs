#![doc = include_str!("../README.md")]

mod bindings;
mod markdown;
mod pages;
mod process;
mod records;
mod report;
mod stored;
mod wait;

use process::Board;
use report::Got;
use std::sync::Arc;

use hangar_core::{
    Binding, BindingName, Blob, MoveError, Page, PageName, PagePin, PageStore, PageStoreError,
    PageVersion, Provenance, PublishedPage, ReadError, Record, RecordBody, RecordPath, RecordStore,
    RecordType, WriteError, WriteProvenance,
};
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

/// The limits a [`BoardStore`] keeps to.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    /// The most board children that run at once. A call that needs another
    /// waits until one ends, for at most `child_deadline`, and then answers
    /// `Unavailable`. The default is 4.
    pub children: usize,
    /// How long one child may run before the store stops it and answers
    /// `Unavailable`. The default is 10 seconds.
    pub child_deadline: Duration,
    /// How long a put waits for the board's lock and Git's locks before it
    /// answers `board::put::locked`, which the store answers as
    /// `Unavailable`. The default is 5 seconds.
    pub lock_wait: Duration,
    /// The most bytes of standard output the store reads from one child. A
    /// child that prints more is stopped, and the store answers
    /// `Unavailable`. The default is 8 MiB.
    pub output_bytes: usize,
    /// How many times a publish or a move reads again and retries after
    /// another writer won the race. After that it answers `Unavailable`.
    /// The default is 3.
    pub retries: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            children: 4,
            child_deadline: Duration::from_secs(10),
            lock_wait: Duration::from_secs(5),
            output_bytes: 8 * 1024 * 1024,
            retries: 3,
        }
    }
}

/// The Git identity that every commit the store makes carries. The person
/// who made a change is named in the records, so this names the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committer {
    name: String,
    email: String,
}

impl Committer {
    /// Builds a committer identity.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError::Committer`] when either part is empty or holds a
    /// control character, `<` or `>`.
    pub fn new(name: &str, email: &str) -> Result<Self, OpenError> {
        let bad = |text: &str| {
            text.is_empty() || text.chars().any(|c| c.is_control() || c == '<' || c == '>')
        };
        if bad(name) || bad(email) {
            return Err(OpenError::Committer);
        }
        Ok(Self {
            name: name.to_owned(),
            email: email.to_owned(),
        })
    }
}

/// Where a [`BoardStore`] finds its board and its board program, and what
/// its children get.
#[derive(Debug, Clone)]
pub struct BoardConfig {
    root: PathBuf,
    program: PathBuf,
    state: PathBuf,
    search_path: OsString,
    committer: Committer,
    limits: Limits,
}

impl BoardConfig {
    /// A configuration with the default [`Limits`]. `root` is the board's
    /// working tree, `program` the board program's absolute path, `state`
    /// the directory that each child gets as `HOME` and that holds its XDG
    /// directories, and `search_path` the `PATH` each child gets, which must
    /// find `git`.
    #[must_use]
    pub fn new(
        root: PathBuf,
        program: PathBuf,
        state: PathBuf,
        search_path: OsString,
        committer: Committer,
    ) -> Self {
        Self {
            root,
            program,
            state,
            search_path,
            committer,
            limits: Limits::default(),
        }
    }

    /// The same configuration with `limits`.
    #[must_use]
    pub fn with_limits(self, limits: Limits) -> Self {
        Self { limits, ..self }
    }
}

/// Why [`BoardStore::open`] refused a configuration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum OpenError {
    /// The root holds no `.board/board.json`.
    #[error("{} holds no .board/board.json", .0.display())]
    NotABoard(PathBuf),
    /// The board program is not an absolute path.
    #[error("the board program {} is not an absolute path", .0.display())]
    RelativeProgram(PathBuf),
    /// The committer identity is empty or holds a character Git refuses.
    #[error("the committer identity is empty or holds a control character, < or >")]
    Committer,
    /// The limits allow no child at all.
    #[error("the limits allow no board child to run")]
    NoChildren,
    /// The board's `.board/board.json` sets `commit_on_put` to false. Each
    /// put on such a board writes files and makes no commit, and every blob
    /// and version the store hands out is a commit.
    #[error("{} sets commit_on_put to false, so its puts make no commit", .0.display())]
    CommitsOff(PathBuf),
    /// The board's `.board/board.json` could not be read or is not a JSON
    /// object.
    #[error("{} could not be read as a JSON object", .0.display())]
    UnreadableConfig(PathBuf),
}

/// Checks the board's `.board/board.json`: it must be a JSON object whose
/// `commit_on_put`, when present, is not false. Board treats an absent
/// `commit_on_put` as true.
fn check_config(root: &std::path::Path) -> Result<(), OpenError> {
    let path = root.join(".board/board.json");
    if !path.is_file() {
        return Err(OpenError::NotABoard(root.to_path_buf()));
    }
    let unreadable = || OpenError::UnreadableConfig(path.clone());
    let bytes = std::fs::read(&path).map_err(|_| unreadable())?;
    let Ok(serde_json::Value::Object(config)) = serde_json::from_slice(&bytes) else {
        return Err(unreadable());
    };
    match config.get("commit_on_put") {
        Some(serde_json::Value::Bool(false)) => Err(OpenError::CommitsOff(path)),
        _ => Ok(()),
    }
}

/// The page and record stores of one board.
///
/// hangar-server opens one and hands it to its handlers as both stores.
/// Every operation runs the board program, at most
/// [`Limits::children`] at once.
#[derive(Debug, Clone)]
pub struct BoardStore {
    board: Arc<Board>,
}

impl BoardStore {
    /// Opens the board that `config` names.
    ///
    /// # Errors
    ///
    /// Returns [`OpenError::NotABoard`] when the root holds no
    /// `.board/board.json`, [`OpenError::UnreadableConfig`] when that file
    /// is not a JSON object, [`OpenError::CommitsOff`] when it sets
    /// `commit_on_put` to false, [`OpenError::RelativeProgram`] when the
    /// board program is not an absolute path and [`OpenError::NoChildren`]
    /// when the limits allow no child.
    ///
    /// A board whose configuration changes to `commit_on_put` false after
    /// it opens still answers `Unavailable` to each write, and the put has
    /// then left its files uncommitted.
    pub fn open(config: BoardConfig) -> Result<Self, OpenError> {
        check_config(&config.root)?;
        if !config.program.is_absolute() {
            return Err(OpenError::RelativeProgram(config.program));
        }
        if config.limits.children == 0 {
            return Err(OpenError::NoChildren);
        }
        let board = Board {
            gate: wait::Gate::new(config.limits.children),
            root: config.root,
            program: config.program,
            state: config.state,
            search_path: config.search_path,
            committer: (config.committer.name, config.committer.email),
            limits: config.limits,
        };
        Ok(Self {
            board: Arc::new(board),
        })
    }

    /// The text of the schema of `record_type`, which is a JSON Schema
    /// 2020-12 document, read from
    /// `/.board/types/<namespace>/<kind>/v<n>.schema`.
    ///
    /// # Errors
    ///
    /// Returns [`ReadError::NotFound`] when the board holds no schema for the
    /// type and [`ReadError::Unavailable`] when the board cannot answer.
    pub fn schema(
        &self,
        record_type: &RecordType,
    ) -> impl Future<Output = Result<String, ReadError>> + Send {
        let path = format!("/.board/types/{record_type}.schema");
        async move {
            let got = self
                .board
                .get(&[path.as_str()])
                .await
                .map_err(|_| ReadError::Unavailable)?;
            match got.into_iter().next() {
                Some(Got::Found(entry)) => entry.text.ok_or(ReadError::Unavailable),
                Some(Got::Missing) => Err(ReadError::NotFound),
                _ => Err(ReadError::Unavailable),
            }
        }
    }
}

impl RecordStore for BoardStore {
    async fn read(&self, path: &RecordPath) -> Result<Record, ReadError> {
        records::read(&self.board, path).await
    }

    async fn read_all(&self, paths: &[RecordPath]) -> Vec<Result<Record, ReadError>> {
        records::read_all(&self.board, paths).await
    }

    async fn write(
        &self,
        path: &RecordPath,
        body: &RecordBody,
        provenance: &WriteProvenance,
    ) -> Result<Blob, WriteError> {
        records::write(&self.board, path, body, provenance).await
    }
}

impl PageStore for BoardStore {
    async fn publish(
        &self,
        page: Page,
        provenance: Provenance,
    ) -> Result<PageVersion, PageStoreError> {
        pages::publish(&self.board, &page, &provenance).await
    }

    async fn latest(&self, name: &PageName) -> Result<PublishedPage, PageStoreError> {
        pages::latest(&self.board, name).await
    }

    async fn at(&self, pin: &PagePin) -> Result<PublishedPage, PageStoreError> {
        pages::at(&self.board, pin).await
    }

    async fn versions(&self, name: &PageName) -> Result<Vec<PageVersion>, PageStoreError> {
        pages::versions(&self.board, name).await
    }

    async fn pages(&self) -> Result<Vec<PageName>, PageStoreError> {
        pages::pages(&self.board).await
    }

    async fn save_binding(
        &self,
        binding: Binding,
        records: &impl RecordStore,
    ) -> Result<(), PageStoreError> {
        bindings::save(&self.board, &binding, records).await
    }

    async fn binding(&self, name: &BindingName) -> Result<Binding, PageStoreError> {
        bindings::read(&self.board, name)
            .await
            .map(|(binding, _)| binding)
    }

    async fn bindings_of(&self, name: &PageName) -> Result<Vec<Binding>, PageStoreError> {
        bindings::of_page(&self.board, name).await
    }

    async fn check_fit(
        &self,
        name: &BindingName,
        records: &impl RecordStore,
    ) -> Result<(), PageStoreError> {
        bindings::check_fit(&self.board, name, records).await
    }

    async fn move_binding(
        &self,
        name: &BindingName,
        expected: &PageVersion,
        version: &PageVersion,
        records: &impl RecordStore,
        mover: &WriteProvenance,
    ) -> Result<(), MoveError> {
        bindings::move_pin(&self.board, name, expected, version, records, mover).await
    }
}

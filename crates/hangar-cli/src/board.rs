//! Finds the board, its board program and hangar's state directories, and
//! opens the board's store.

use crate::codes::Failed;
use crate::env::Snapshot;
use hangar_auth_loopback::StateDir;
use hangar_board::{BoardConfig, BoardStore, Committer, OpenError, ProgramError};
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};

/// The file whose presence makes a directory a board.
const MARKER: &str = ".board/board.json";

/// How a command names the board.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Need {
    /// The directory must hold a board.
    Board,
    /// Any directory works, because the command reads only hangar's state
    /// for it, as `hangar open` does.
    Directory,
}

/// The board's canonical directory: `given`, else the nearest directory at
/// or above the working directory that holds `.board/board.json`.
///
/// # Errors
///
/// Returns [`Failed::BoardNotFound`] when `given` does not exist and
/// [`Failed::NotABoard`] when the directory holds no board.
pub(crate) fn root(given: Option<&Path>, need: Need, env: &Snapshot) -> Result<PathBuf, Failed> {
    let start = match given {
        Some(given) => given.to_path_buf(),
        None => env
            .current_dir()
            .cloned()
            .ok_or_else(|| Failed::BoardNotFound {
                path: "the working directory".to_owned(),
            })?,
    };
    let canonical = std::fs::canonicalize(&start).map_err(|_| Failed::BoardNotFound {
        path: start.display().to_string(),
    })?;
    if given.is_none() {
        // Without --board, the nearest board above the working directory,
        // as git finds its repository.
        if let Some(found) = canonical.ancestors().find(|dir| dir.join(MARKER).is_file()) {
            return Ok(found.to_path_buf());
        }
    }
    match need {
        Need::Directory => Ok(canonical),
        Need::Board if canonical.join(MARKER).is_file() => Ok(canonical),
        Need::Board => Err(Failed::NotABoard {
            path: canonical.display().to_string(),
        }),
    }
}

/// The state home: `XDG_STATE_HOME`, else `~/.local/state`.
///
/// # Errors
///
/// Returns [`Failed::StateUnusable`] when neither is an absolute path.
pub(crate) fn state_home(env: &Snapshot) -> Result<PathBuf, Failed> {
    hangar_auth_loopback::state_home(env.get("XDG_STATE_HOME"), env.get("HOME")).map_err(|error| {
        Failed::StateUnusable {
            problem: error.to_string(),
        }
    })
}

/// The loopback state directory of the board at `root`.
///
/// # Errors
///
/// As [`state_home`].
pub(crate) fn loopback_state(root: &Path, env: &Snapshot) -> Result<StateDir, Failed> {
    Ok(StateDir::for_board(&state_home(env)?, root))
}

/// The board program: `given`, else `HANGAR_BOARD`, else the first `board`
/// on `PATH`, made absolute.
///
/// # Errors
///
/// Returns [`Failed::ProgramUnavailable`] when none is found.
pub(crate) fn program(given: Option<&Path>, env: &Snapshot) -> Result<PathBuf, Failed> {
    let unavailable = |problem: String| Failed::ProgramUnavailable { problem };
    let named = given
        .map(Path::to_path_buf)
        .or_else(|| env.get("HANGAR_BOARD").map(PathBuf::from));
    let found = match named {
        Some(path) => path,
        None => std::env::split_paths(&env.search_path())
            .map(|dir| dir.join("board"))
            .find(|candidate| candidate.is_file())
            .ok_or_else(|| unavailable("board was not found on PATH".to_owned()))?,
    };
    std::fs::canonicalize(&found)
        .map_err(|error| unavailable(format!("{}: {error}", found.display())))
}

/// Opens the store of the board at `root` with the board program
/// `program`. Each board child gets `<state>/hangar/board/<key>/` as its
/// `HOME`, made with mode 0700, and commits as `hangar <hangar@localhost>`.
///
/// # Errors
///
/// Returns [`Failed::NotABoard`], [`Failed::StateUnusable`] or
/// [`Failed::BoardUnavailable`].
pub(crate) fn open(root: &Path, program: PathBuf, env: &Snapshot) -> Result<BoardStore, Failed> {
    let loopback = loopback_state(root, env)?;
    let key = loopback
        .path()
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_default();
    let children = state_home(env)?.join("hangar").join("board").join(key);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&children)
        .map_err(|error| Failed::StateUnusable {
            problem: format!("{}: {error}", children.display()),
        })?;
    let committer = Committer::new("hangar", "hangar@localhost").map_err(unavailable)?;
    let config = BoardConfig::new(
        root.to_path_buf(),
        program,
        children,
        env.search_path(),
        committer,
    );
    BoardStore::open(config).map_err(|error| match error {
        OpenError::NotABoard(path) => Failed::NotABoard {
            path: path.display().to_string(),
        },
        other => unavailable(other),
    })
}

/// Checks that the store's board program can read history.
///
/// # Errors
///
/// Returns [`Failed::TooOld`] or [`Failed::ProgramUnavailable`].
pub(crate) async fn check(store: &BoardStore, program: &Path) -> Result<(), Failed> {
    store.check_program().await.map_err(|error| match error {
        ProgramError::TooOld => Failed::TooOld {
            program: program.display().to_string(),
        },
        _ => Failed::ProgramUnavailable {
            problem: format!("at {} did not answer", program.display()),
        },
    })
}

/// A store and its program, opened and checked: what `serve`, `publish`
/// and `bind` start with.
///
/// # Errors
///
/// As [`root`], [`program`], [`open`] and [`check`].
pub(crate) async fn checked_store(
    given: &crate::cli::BoardArgs,
    env: &Snapshot,
) -> Result<(PathBuf, BoardStore), Failed> {
    let root = root(given.board.as_deref(), Need::Board, env)?;
    let program = program(given.board_program.as_deref(), env)?;
    let store = open(&root, program.clone(), env)?;
    check(&store, &program).await?;
    Ok((root, store))
}

fn unavailable(error: impl std::fmt::Display) -> Failed {
    Failed::BoardUnavailable {
        problem: error.to_string(),
    }
}

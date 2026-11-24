//! Runs the board program. This is the crate's one process module (PROC-1),
//! with one function per board operation: `get`, `get --at`, `ls`, `log`
//! and `put --batch`.
//!
//! Each operation, in PROC-2's five items:
//!
//! - **Program.** The board program at the absolute path that
//!   [`BoardConfig`](crate::BoardConfig) names, never found through `PATH`.
//!   The person who configures the store supplies it.
//! - **Arguments.** Separate values with no shell: the subcommand, its
//!   options, then `--`, then the logical paths. Every path, name and
//!   version reached its child only after flare-core's types parsed it, so
//!   no value after `--` is read as an option and none holds a control
//!   character.
//! - **Environment.** Cleared, then `PATH` as configured, `HOME` set to the
//!   configured state directory, `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and
//!   `XDG_STATE_HOME` inside it, `GIT_CONFIG_NOSYSTEM=1` so Git reads no
//!   system configuration, and the committer identity as `GIT_AUTHOR_*` and
//!   `GIT_COMMITTER_*`. The working directory is the board's root.
//! - **Streams and deadline.** Standard input is empty, or the batch's JSON
//!   Lines. Standard error is discarded. At most
//!   [`Limits::output_bytes`](crate::Limits::output_bytes) of standard
//!   output are read, and a child that prints more is stopped. The deadline
//!   is [`Limits::child_deadline`](crate::Limits::child_deadline) from the
//!   moment the child gets its place in the gate.
//! - **End.** crumbkit-process starts the child in a process group of its
//!   own and, at the deadline or the output limit, sends the group SIGTERM,
//!   waits one second, kills it and reaps the child. It does the same when
//!   the caller drops the call's future, and the drop returns only once
//!   the child is reaped (`wait.rs`). A put that is stopped
//!   may leave written files with no commit, which the next write replaces,
//!   as the lifecycle's crash points say. The exit code must agree with the
//!   report, as [`parse`] checks.
//!
//! A counting gate lets at most [`Limits::children`](crate::Limits::children)
//! children run at once. A call that finds no free place waits on the
//! thread that runs it for at most
//! [`Limits::child_deadline`](crate::Limits::child_deadline), and then fails
//! as a child failure, which the stores answer as `Unavailable`. The place
//! is freed when that thread ends.

use crate::Limits;
use crate::report::{Failure, GetReport, Got, LogReport, LsReport, LsRow, PutReport, Ran, parse};
use crate::wait::{Gate, on_thread};
use crumbkit_process::{
    Cancel, Capture, Escalation, Input, Outcome, RunOptions, Signal, Status, StopReason, Zeroizing,
};
use serde_json::Value;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long a stopped child has between SIGTERM and SIGKILL. Board removes
/// its lock files on SIGTERM.
const TERM_GRACE: Duration = Duration::from_secs(1);

/// The board program, the board it works on and what each child gets.
#[derive(Debug)]
pub(crate) struct Board {
    pub(crate) root: PathBuf,
    pub(crate) program: PathBuf,
    pub(crate) state: PathBuf,
    pub(crate) search_path: OsString,
    pub(crate) committer: (String, String),
    pub(crate) limits: Limits,
    pub(crate) gate: Gate,
}

/// How many rows a `board log` lists.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Rows {
    /// The newest one.
    Newest,
    /// Every row.
    All,
}

impl Board {
    /// Reads `paths` in the working tree. The answer has one entry per path.
    pub(crate) async fn get(self: &Arc<Self>, paths: &[&str]) -> Result<Vec<Got>, Failure> {
        let mut args = vec!["get", "--json", "--"];
        args.extend_from_slice(paths);
        let report: GetReport = parse(&self.run(&args, Vec::new()).await?, "board/get-report/v2")?;
        report.per_path(paths.len())
    }

    /// Reads `paths` in the tree of `commit`, `HEAD` or a full commit id.
    pub(crate) async fn get_at(
        self: &Arc<Self>,
        commit: &str,
        paths: &[&str],
    ) -> Result<Vec<Got>, Failure> {
        let mut args = vec!["get", "--at", commit, "--json", "--"];
        args.extend_from_slice(paths);
        let report: GetReport = parse(&self.run(&args, Vec::new()).await?, "board/get-report/v2")?;
        report.per_path(paths.len())
    }

    /// Lists the directory `directory`, or answers `None` when nothing is
    /// there.
    pub(crate) async fn ls(
        self: &Arc<Self>,
        directory: &str,
    ) -> Result<Option<Vec<LsRow>>, Failure> {
        let args = ["ls", "--json", "--all", "--", directory];
        match parse::<LsReport>(&self.run(&args, Vec::new()).await?, "board/ls-report/v2") {
            Ok(report) if report.truncated => Err(Failure::Child("ls stopped early".into())),
            Ok(report) => Ok(Some(report.data)),
            Err(Failure::Board(failure)) if failure.code == "board::path::not_found" => Ok(None),
            Err(other) => Err(other),
        }
    }

    /// Lists the commits that changed `path`, newest first, walking from
    /// `commit`, or from `HEAD` when it is `None`.
    pub(crate) async fn log(
        self: &Arc<Self>,
        path: &str,
        commit: Option<&str>,
        rows: Rows,
    ) -> Result<LogReport, Failure> {
        let mut args = vec!["log", "--json"];
        if let Some(commit) = commit {
            args.extend(["--at", commit]);
        }
        match rows {
            Rows::Newest => args.extend(["--limit", "1"]),
            Rows::All => args.push("--all"),
        }
        args.extend(["--", path]);
        parse(&self.run(&args, Vec::new()).await?, "board/log-report/v2")
    }

    /// Writes the JSON Lines `lines` with one `board put --batch`, which
    /// applies all of them or none.
    pub(crate) async fn put_batch(self: &Arc<Self>, lines: &[Value]) -> Result<PutReport, Failure> {
        let mut input = Vec::new();
        for line in lines {
            input.extend_from_slice(line.to_string().as_bytes());
            input.push(b'\n');
        }
        let wait = format!("{}ms", self.limits.lock_wait.as_millis().max(1));
        let args = ["put", "--batch", "--json", "--wait", wait.as_str()];
        parse(&self.run(&args, input).await?, "board/put-report/v2")
    }

    /// Runs board with `args` and `stdin` on a thread of its own and
    /// returns its standard output and exit code. Dropping the future
    /// stops the child and waits for it to end.
    async fn run(self: &Arc<Self>, args: &[&str], stdin: Vec<u8>) -> Result<Ran, Failure> {
        let board = Arc::clone(self);
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let stop = Cancel::new();
        let watched = stop.clone();
        on_thread(stop, move || board.run_blocking(args, stdin, &watched))
            .await
            .unwrap_or_else(|| {
                Err(Failure::Child(
                    "the thread that ran board ended early".into(),
                ))
            })
    }

    fn run_blocking(
        &self,
        args: Vec<OsString>,
        stdin: Vec<u8>,
        stop: &Cancel,
    ) -> Result<Ran, Failure> {
        let deadline = || {
            Instant::now()
                .checked_add(self.limits.child_deadline)
                .ok_or_else(|| Failure::Child("the child deadline is too far away".into()))
        };
        let _place = self.gate.enter_within(deadline()?, stop).ok_or_else(|| {
            Failure::Child(format!(
                "no place in the gate freed within {:?}",
                self.limits.child_deadline
            ))
        })?;
        #[expect(
            clippy::disallowed_methods,
            reason = "this is flare-board's process module (PROC-1)"
        )]
        let mut command = Command::new(&self.program);
        let (name, email) = &self.committer;
        command
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", &self.search_path)
            .env("HOME", &self.state)
            .env("XDG_CONFIG_HOME", self.state.join("config"))
            .env("XDG_DATA_HOME", self.state.join("data"))
            .env("XDG_STATE_HOME", self.state.join("state"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", name)
            .env("GIT_AUTHOR_EMAIL", email)
            .env("GIT_COMMITTER_NAME", name)
            .env("GIT_COMMITTER_EMAIL", email);
        let input = if stdin.is_empty() {
            Input::Null
        } else {
            Input::Bytes(Zeroizing::new(stdin))
        };
        let options = RunOptions::new()
            .stdin(input)
            .stdout(Capture::whole(self.limits.output_bytes))
            .stderr(Capture::discard())
            .deadline(deadline()?)
            .cancel(stop)
            .escalation(Escalation::default().step(Signal::Terminate, TERM_GRACE));
        let finished = crumbkit_process::run(command, &options)
            .map_err(|error| Failure::Child(format!("board could not run: {error}")))?;
        let code = ended(
            finished.outcome(),
            finished.stdout().over_limit(),
            &self.limits,
        )?;
        Ok(Ran {
            stdout: finished.into_output().0.into_bytes(),
            code,
        })
    }
}

/// The exit code of a child that ended with `outcome`, or why its output
/// cannot be used. `over_limit` says whether it printed past
/// [`Limits::output_bytes`].
pub(crate) fn ended(outcome: Outcome, over_limit: bool, limits: &Limits) -> Result<i32, Failure> {
    let over = || {
        Failure::Child(format!(
            "board printed more than {} bytes",
            limits.output_bytes
        ))
    };
    match outcome {
        // A child that exits on its own just after it passes the limit
        // ends before the run stops it, with its output cut short.
        Outcome::Ended(_) if over_limit => Err(over()),
        Outcome::Ended(Status::Code(code)) => Ok(code),
        Outcome::Ended(status) => Err(Failure::Child(format!(
            "board ended by a signal, with status {status:?}"
        ))),
        Outcome::TimedOut => Err(Failure::Child(format!(
            "board ran past its deadline of {:?}",
            limits.child_deadline
        ))),
        Outcome::Stopped(StopReason::Cancelled) => Err(Failure::Child(
            "the call was dropped, so its child was stopped".into(),
        )),
        _ => Err(over()),
    }
}

#[cfg(test)]
mod tests;

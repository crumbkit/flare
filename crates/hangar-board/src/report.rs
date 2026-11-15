//! The JSON reports that the board program prints, and its failure codes.
//!
//! Each report names its schema in `schema`. A report of another schema
//! than the one an operation expects, or one that does not parse, is a
//! [`Failure::Child`], which the stores answer as `Unavailable`.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// What went wrong with one board operation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Failure {
    /// The board program answered with `board/failure/v1`.
    Board(BoardFailure),
    /// The child could not start, ran past its deadline, printed more than
    /// the output limit, or printed something that is not the report the
    /// operation expects. The text says which.
    Child(String),
}

/// A `board/failure/v1` report.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct BoardFailure {
    pub(crate) code: String,
    #[serde(default)]
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) fields: Value,
}

impl BoardFailure {
    /// The refusals of a batch, each with its line and code, or this
    /// failure alone when it lists none.
    pub(crate) fn refusals(&self) -> Vec<Refusal> {
        let listed = self
            .fields
            .get("refusals")
            .and_then(|refusals| Vec::<Refusal>::deserialize(refusals).ok());
        listed.unwrap_or_else(|| {
            vec![Refusal {
                code: self.code.clone(),
                line: 1,
                message: self.message.clone(),
                fields: self.fields.clone(),
            }]
        })
    }
}

/// One refused line of a batch. `line` counts from 1.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct Refusal {
    pub(crate) code: String,
    #[serde(default)]
    pub(crate) line: usize,
    #[serde(default)]
    pub(crate) message: String,
    #[serde(default)]
    pub(crate) fields: Value,
}

impl Refusal {
    /// For a stale expect, the blob the file holds, or `None` when the path
    /// is empty. `None` for any other refusal.
    pub(crate) fn stale_found(&self) -> Option<Option<String>> {
        (self.code == "board::put::stale").then(|| {
            self.fields
                .get("found")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
    }
}

/// Where a path resolved.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Location {
    /// The file's path in its repository.
    pub(crate) path: String,
    /// The links and mounts the resolution followed.
    #[serde(default)]
    pub(crate) pointers: Vec<Value>,
    /// `worktree`, or the id of the commit the file was read at.
    pub(crate) source: String,
}

/// One file that `board get` answered.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct Entry {
    pub(crate) location: Location,
    pub(crate) blob: String,
    #[serde(default)]
    pub(crate) text: Option<String>,
    #[serde(default)]
    pub(crate) record: Option<Value>,
}

/// One path that `board get` could not answer.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct Missed {
    pub(crate) index: usize,
    pub(crate) code: String,
}

/// `board/get-report/v2`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct GetReport {
    pub(crate) data: Vec<Entry>,
    #[serde(default)]
    pub(crate) failures: Vec<Missed>,
}

/// What `board get` answered for one of the paths it was given.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Got {
    Found(Entry),
    Missing,
    /// The path failed with another code than `board::path::not_found`.
    Failed(String),
}

impl GetReport {
    /// One answer per path, in the order of the paths given. The report
    /// lists the files found in that order and names the index of each
    /// failure, so the two lists are merged by position.
    pub(crate) fn per_path(self, count: usize) -> Result<Vec<Got>, Failure> {
        let mut found = self.data.into_iter();
        let mut answers = Vec::with_capacity(count);
        for index in 0..count {
            let missed = self.failures.iter().find(|missed| missed.index == index);
            answers.push(match missed {
                Some(missed) if missed.code == "board::path::not_found" => Got::Missing,
                Some(missed) => Got::Failed(missed.code.clone()),
                None => Got::Found(
                    found
                        .next()
                        .ok_or_else(|| Failure::Child("get answered fewer files".into()))?,
                ),
            });
        }
        Ok(answers)
    }
}

/// One commit that `board log` listed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct LogRow {
    pub(crate) commit: String,
    #[serde(default)]
    pub(crate) blob: Option<String>,
}

/// `board/log-report/v2`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct LogReport {
    pub(crate) data: Vec<LogRow>,
    #[serde(default)]
    pub(crate) truncated: bool,
}

/// One entry that `board ls` listed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct LsRow {
    pub(crate) kind: String,
    pub(crate) logical: String,
}

/// `board/ls-report/v2`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct LsReport {
    pub(crate) data: Vec<LsRow>,
    #[serde(default)]
    pub(crate) truncated: bool,
}

/// The `data` of `board/put-report/v2`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct PutData {
    /// The commit the put made, or `None` when the board does not commit
    /// its puts or nothing changed.
    pub(crate) commit: Option<String>,
}

/// `board/put-report/v2`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct PutReport {
    pub(crate) data: PutData,
}

/// Parses a child's standard output as the report of `schema`, or as the
/// failure the board answered.
pub(crate) fn parse<T: DeserializeOwned>(stdout: &[u8], schema: &str) -> Result<T, Failure> {
    let value: Value = serde_json::from_slice(stdout)
        .map_err(|_| Failure::Child(format!("the answer to a {schema} read is not JSON")))?;
    let named = value.get("schema").and_then(Value::as_str);
    if named == Some("board/failure/v1") {
        let failure = BoardFailure::deserialize(&value)
            .map_err(|_| Failure::Child("a board failure report does not parse".into()))?;
        return Err(Failure::Board(failure));
    }
    if named != Some(schema) {
        return Err(Failure::Child(format!(
            "expected a {schema} report and got {named:?}"
        )));
    }
    T::deserialize(&value).map_err(|_| Failure::Child(format!("a {schema} report does not parse")))
}

#[cfg(test)]
mod tests;

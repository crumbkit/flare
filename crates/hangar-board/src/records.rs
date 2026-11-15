//! [`RecordStore`](hangar_core::RecordStore) over a board: reading one
//! record, reading several at one commit, and writing one with its edit
//! record.
//!
//! A record's hangar [`Blob`] is the commit that last changed its file when
//! the working tree holds that commit's bytes, and the Git blob id of the
//! working-tree bytes otherwise. A commit id never repeats, so a record that
//! returns to earlier bytes gets a new blob, as the contract requires.

use crate::markdown::{self, MarkdownError};
use crate::process::{Board, Rows};
use crate::report::{Entry, Failure, Got};
use crate::stored::{self, EDIT_TYPE};
use hangar_core::{
    Blob, ReadError, Record, RecordBody, RecordPath, RecordType, Rejection, WriteError,
    WriteProvenance,
};
use serde_json::{Map, Value, json};
use std::sync::Arc;

/// The directories that only the page store writes. A record edit under one
/// of them is rejected, so a form cannot change a page, a binding, an edit
/// record or a schema behind the page store.
const RESERVED: [&str; 4] = ["/.board/", "/pages/", "/bindings/", "/hangar/"];

/// One record as a read found it, with what a write needs.
struct Current {
    record: Record,
    /// The Git blob id of the working-tree bytes, which `--expect` takes.
    file_blob: String,
    /// The record's JSON object, including `$body` for Markdown.
    object: Map<String, Value>,
    /// For a Markdown record, the front-matter keys in the file's order.
    order: Option<Vec<String>>,
}

fn is_markdown(path: &RecordPath) -> bool {
    path.as_str().ends_with(".md")
}

/// A record's JSON object, its text and, for Markdown, its field order.
struct Shaped {
    object: Map<String, Value>,
    body: String,
    order: Option<Vec<String>>,
}

/// The record's text: board's own JSON text, or for a Markdown record the
/// JSON object of its front matter and prose, pretty-printed with sorted
/// keys and a final newline.
fn shape(path: &RecordPath, entry: &Entry) -> Result<Shaped, ReadError> {
    let Some(Value::Object(fields)) = entry.record.clone() else {
        return Err(ReadError::Unavailable);
    };
    let text = entry.text.as_deref().ok_or(ReadError::Unavailable)?;
    if !is_markdown(path) {
        return Ok(Shaped {
            object: fields,
            body: text.to_owned(),
            order: None,
        });
    }
    let (block, prose) = markdown::split(text).ok_or(ReadError::Unavailable)?;
    let order = markdown::field_order(block);
    let object = markdown::to_object(fields, prose);
    let mut body = serde_json::to_string_pretty(&sorted(Value::Object(object.clone())))
        .map_err(|_| ReadError::Unavailable)?;
    body.push('\n');
    Ok(Shaped {
        object,
        body,
        order: Some(order),
    })
}

/// `value` with the keys of every object in sorted order, whichever order
/// serde_json's map keeps.
fn sorted(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, inner)| (key, sorted(inner)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        other => other,
    }
}

/// Builds the record that `entry` holds, with `blob` as its hangar blob.
fn current(path: &RecordPath, entry: Entry, blob: &str) -> Result<Current, ReadError> {
    if !entry.location.pointers.is_empty() {
        // Shortcut: a record reached through a link or a mount answers
        // Unavailable in this version, because its history lives in
        // another place than its path names.
        return Err(ReadError::Unavailable);
    }
    let Shaped {
        object,
        body,
        order,
    } = shape(path, &entry)?;
    let record_type = object
        .get("$type")
        .and_then(Value::as_str)
        .and_then(|text| RecordType::try_from(text).ok())
        .ok_or(ReadError::Unavailable)?;
    let record = Record::new(
        path.clone(),
        record_type,
        Blob::try_from(blob).map_err(|_| ReadError::Unavailable)?,
        RecordBody::try_from(body.as_str()).map_err(|_| ReadError::Unavailable)?,
    );
    Ok(Current {
        record,
        file_blob: entry.blob,
        object,
        order,
    })
}

fn unavailable<T>(_failure: Failure) -> Result<T, ReadError> {
    Err(ReadError::Unavailable)
}

/// Reads the record at `path` in the working tree.
async fn read_current(board: &Arc<Board>, path: &RecordPath) -> Result<Current, ReadError> {
    let got = board.get(&[path.as_str()]).await.or_else(unavailable)?;
    let entry = match got.into_iter().next() {
        Some(Got::Found(entry)) => entry,
        Some(Got::Missing) => return Err(ReadError::NotFound),
        _ => return Err(ReadError::Unavailable),
    };
    let log = board
        .log(path.as_str(), None, Rows::Newest)
        .await
        .or_else(unavailable)?;
    let blob = match log.data.first() {
        Some(row) if row.blob.as_deref() == Some(entry.blob.as_str()) => row.commit.clone(),
        _ => entry.blob.clone(),
    };
    current(path, entry, &blob)
}

pub(crate) async fn read(board: &Arc<Board>, path: &RecordPath) -> Result<Record, ReadError> {
    read_current(board, path).await.map(|found| found.record)
}

/// Reads every path in the tree of one commit, the one `HEAD` names when
/// `board get --at HEAD` runs. Each found record's blob is the commit that
/// last changed it as of that commit, which `board log --at` answers. A
/// commit that lands meanwhile changes neither answer.
pub(crate) async fn read_all(
    board: &Arc<Board>,
    paths: &[RecordPath],
) -> Vec<Result<Record, ReadError>> {
    let unavailable_all = || paths.iter().map(|_| Err(ReadError::Unavailable)).collect();
    if paths.is_empty() {
        return Vec::new();
    }
    let texts: Vec<&str> = paths.iter().map(RecordPath::as_str).collect();
    let Ok(answers) = board.get_at("HEAD", &texts).await else {
        return unavailable_all();
    };
    let commits: Vec<&str> = answers
        .iter()
        .filter_map(|got| match got {
            Got::Found(entry) => Some(entry.location.source.as_str()),
            _ => None,
        })
        .collect();
    let commit = commits.first().map(|commit| (*commit).to_owned());
    if commits
        .iter()
        .any(|other| Some(*other) != commit.as_deref())
    {
        return unavailable_all();
    }
    let mut records = Vec::with_capacity(paths.len());
    for (path, got) in paths.iter().zip(answers) {
        records.push(match (got, &commit) {
            (Got::Found(entry), Some(commit)) => at_commit(board, path, entry, commit).await,
            (Got::Missing, _) => Err(ReadError::NotFound),
            _ => Err(ReadError::Unavailable),
        });
    }
    records
}

async fn at_commit(
    board: &Arc<Board>,
    path: &RecordPath,
    entry: Entry,
    commit: &str,
) -> Result<Record, ReadError> {
    let log = board
        .log(path.as_str(), Some(commit), Rows::Newest)
        .await
        .or_else(unavailable)?;
    match log.data.first() {
        Some(row) if row.blob.as_deref() == Some(entry.blob.as_str()) => {
            let blob = row.commit.clone();
            current(path, entry, &blob).map(|found| found.record)
        }
        _ => Err(ReadError::Unavailable),
    }
}

fn rejected(text: impl Into<String>) -> WriteError {
    WriteError::Rejected(Rejection::new(text))
}

/// Replaces the record at `path` with `body`, and in the same commit adds a
/// `hangar/edit/v1` record of the edit.
///
/// Known gap: when two other writers move the record's bytes away and back
/// between this read and the put, the put succeeds, because the bytes equal
/// what the edit was based on. The edit record still names the true
/// `based_on`.
pub(crate) async fn write(
    board: &Arc<Board>,
    path: &RecordPath,
    body: &RecordBody,
    provenance: &WriteProvenance,
) -> Result<Blob, WriteError> {
    let found = read_current(board, path)
        .await
        .map_err(|error| match error {
            ReadError::NotFound => WriteError::NotFound,
            _ => WriteError::Unavailable,
        })?;
    if found.record.blob() != provenance.based_on() {
        return Err(WriteError::Conflict);
    }
    if RESERVED
        .iter()
        .any(|prefix| path.as_str().starts_with(prefix))
    {
        return Err(rejected("hangar keeps the records under this path itself"));
    }
    let Ok(Value::Object(object)) = serde_json::from_str::<Value>(body.as_str()) else {
        return Err(rejected("a record's body is one JSON object"));
    };
    if object == found.object {
        return Ok(found.record.blob().clone());
    }
    let line = match &found.order {
        Some(order) => {
            let file = markdown::from_object(&object, order)
                .map_err(|error: MarkdownError| rejected(error.text()))?;
            json!({"path": path.as_str(), "expect": found.file_blob, "text": file})
        }
        None => json!({"path": path.as_str(), "expect": found.file_blob, "record": object}),
    };
    let edit = json!({"new": "/hangar/edits/", "record": stored::edit_record(path, provenance)});
    let schema = stored::schema_line(EDIT_TYPE).map_err(|_| WriteError::Unavailable)?;
    match board.put_batch(&[line, edit, schema]).await {
        Ok(report) => report
            .data
            .commit
            .and_then(|commit| Blob::try_from(commit.as_str()).ok())
            .ok_or(WriteError::Unavailable),
        Err(Failure::Board(failure)) => {
            let refusal = failure
                .refusals()
                .into_iter()
                .find(|refusal| refusal.line == 1);
            match refusal {
                Some(refusal) => match refusal.stale_found() {
                    Some(None) => Err(WriteError::NotFound),
                    Some(Some(_)) => Err(WriteError::Conflict),
                    None if refusal.code == "board::put::symlink" => Err(rejected(
                        "the record's file is a symlink, and board writes through none",
                    )),
                    None if refuses_content(&refusal.code) => {
                        confirm_rejection(board, path, provenance, refusal.message).await
                    }
                    None => Err(WriteError::Unavailable),
                },
                None => Err(WriteError::Unavailable),
            }
        }
        Err(Failure::Child(_)) => Err(WriteError::Unavailable),
    }
}

/// The codes with which board refuses a record's content.
fn refuses_content(code: &str) -> bool {
    code.starts_with("board::record::")
        || code.starts_with("board::frontmatter::")
        || code == "board::json::parse"
}

/// Board checks a record's schema before its expect, so a refusal of the
/// content answers `Rejected` only when the record still holds the blob the
/// write was based on, and `Conflict` otherwise.
async fn confirm_rejection(
    board: &Arc<Board>,
    path: &RecordPath,
    provenance: &WriteProvenance,
    message: String,
) -> Result<Blob, WriteError> {
    match read_current(board, path).await {
        Ok(found) if found.record.blob() == provenance.based_on() => Err(rejected(message)),
        Ok(_) => Err(WriteError::Conflict),
        Err(ReadError::NotFound) => Err(WriteError::NotFound),
        Err(_) => Err(WriteError::Unavailable),
    }
}

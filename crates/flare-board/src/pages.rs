//! Page versions: publishing one, reading one, and listing them.
//!
//! A page lives under `/pages/<name>/`: its text in `page.html` or
//! `page.md`, and its `flare/page/v1` record in `page.json`. Each publish is
//! one batch that changes `page.json`, because the record names the version
//! it replaced in `previous`, so every publish makes a commit. That commit's
//! id is the version's name.

use crate::bindings;
use crate::process::{Board, Rows};
use crate::report::{Failure, Got, LogRow};
use crate::stored::{self, PAGE_TYPE, PageRecord};
use flare_core::{
    ContentFormat, MadeFrom, Page, PageName, PagePin, PageStoreError, PageVersion, Provenance,
    PublishedPage,
};
use serde_json::{Value, json};
use std::sync::Arc;

/// The logical path of a page's record.
fn record_path(name: &PageName) -> String {
    format!("/pages/{name}/page")
}

/// The logical path of a page's text in `format`.
fn content_path(name: &PageName, format: ContentFormat) -> String {
    format!("/pages/{name}/{}", stored::content_file(format))
}

fn unavailable<T>(_failure: Failure) -> Result<T, PageStoreError> {
    Err(PageStoreError::Unavailable)
}

/// Whether `text` is a full commit id, the only form a version takes.
fn is_commit(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// The commits that changed the page's record, newest first. A walk that
/// stopped after 100,000 commits or reached a shallow clone's boundary
/// answers `Unavailable`, because its list may miss versions or name the
/// boundary as one.
pub(crate) async fn versions(
    board: &Arc<Board>,
    name: &PageName,
) -> Result<Vec<PageVersion>, PageStoreError> {
    match board.log(&record_path(name), None, Rows::All).await {
        Ok(log) if log.truncated || !log.summary.complete => Err(PageStoreError::Unavailable),
        Ok(log) => {
            let mut versions = Vec::with_capacity(log.data.len());
            for row in log.data {
                versions.push(
                    PageVersion::try_from(row.commit.as_str())
                        .map_err(|_| PageStoreError::Unavailable)?,
                );
            }
            Ok(versions)
        }
        Err(Failure::Board(failure)) if failure.code == "board::path::not_found" => {
            Err(PageStoreError::NotFound)
        }
        Err(other) => unavailable(other),
    }
}

/// The newest row of the page record's log from `HEAD`, which a version
/// needs a complete walk to trust. No row from a complete walk is
/// `NotFound`. A walk that stopped after 100,000 commits or reached a
/// shallow clone's boundary is `Unavailable`, because the row it lists, or
/// the row it missed, may not be the page's last change.
async fn newest(board: &Arc<Board>, name: &PageName) -> Result<LogRow, PageStoreError> {
    match board.log(&record_path(name), None, Rows::Newest).await {
        Ok(log) if !log.summary.complete => Err(PageStoreError::Unavailable),
        Ok(log) => log.data.into_iter().next().ok_or(PageStoreError::NotFound),
        Err(Failure::Board(failure)) if failure.code == "board::path::not_found" => {
            Err(PageStoreError::NotFound)
        }
        Err(other) => unavailable(other),
    }
}

/// The newest version of a page.
pub(crate) async fn latest(
    board: &Arc<Board>,
    name: &PageName,
) -> Result<PublishedPage, PageStoreError> {
    let row = newest(board, name).await?;
    let version =
        PageVersion::try_from(row.commit.as_str()).map_err(|_| PageStoreError::Unavailable)?;
    read_version(board, name, &version).await
}

/// One version of a page. A version is only a commit that changed the
/// page's record, so a commit that never changed it is not found.
///
/// One `board log --at <version> --limit 1` decides it: its first row is
/// the version itself exactly when the version changed the record, and the
/// walk reads that one commit, however long the history. When the walk is
/// not complete, the version is a shallow clone's boundary, where every
/// file looks added, and the answer is `Unavailable`. A commit the board
/// does not hold is not found.
pub(crate) async fn at(board: &Arc<Board>, pin: &PagePin) -> Result<PublishedPage, PageStoreError> {
    let version = pin.version().as_str();
    if !is_commit(version) {
        return Err(PageStoreError::NotFound);
    }
    let log = match board
        .log(&record_path(pin.name()), Some(version), Rows::Newest)
        .await
    {
        Ok(log) => log,
        Err(Failure::Board(failure))
            if failure.code == "board::path::not_found"
                || failure.code == "board::commit::not_found" =>
        {
            return Err(PageStoreError::NotFound);
        }
        Err(other) => return unavailable(other),
    };
    match log.data.first() {
        Some(row) if row.commit == version && row.blob.is_some() => {
            if !log.summary.complete {
                return Err(PageStoreError::Unavailable);
            }
        }
        _ => return Err(PageStoreError::NotFound),
    }
    read_version(board, pin.name(), pin.version()).await
}

/// Reads the record and the text of the page at `version`.
async fn read_version(
    board: &Arc<Board>,
    name: &PageName,
    version: &PageVersion,
) -> Result<PublishedPage, PageStoreError> {
    let paths = [
        record_path(name),
        content_path(name, ContentFormat::Html),
        content_path(name, ContentFormat::Markdown),
    ];
    let texts: Vec<&str> = paths.iter().map(String::as_str).collect();
    let got = board
        .get_at(version.as_str(), &texts)
        .await
        .or_else(unavailable)?;
    let mut got = got.into_iter();
    let record = match got.next() {
        Some(Got::Found(entry)) => entry.record.ok_or(PageStoreError::Unavailable)?,
        Some(Got::Missing) => return Err(PageStoreError::NotFound),
        _ => return Err(PageStoreError::Unavailable),
    };
    let record: PageRecord =
        stored::from_value(&record).map_err(|_| PageStoreError::Unavailable)?;
    let format = record.format().map_err(|_| PageStoreError::Unavailable)?;
    let (html, markdown) = (got.next(), got.next());
    let content = match format {
        ContentFormat::Markdown => markdown,
        _ => html,
    };
    let Some(Got::Found(content)) = content else {
        return Err(PageStoreError::Unavailable);
    };
    let text = content.text.ok_or(PageStoreError::Unavailable)?;
    let (page, provenance) = record
        .page(name, &text)
        .map_err(|_| PageStoreError::Unavailable)?;
    Ok(PublishedPage::new(page, version.clone(), provenance))
}

/// Answers `Ok` when the page's record is in the working tree, and
/// `NotFound` when it is not.
pub(crate) async fn exists(board: &Arc<Board>, name: &PageName) -> Result<(), PageStoreError> {
    let got = board
        .get(&[record_path(name).as_str()])
        .await
        .or_else(unavailable)?;
    match got.into_iter().next() {
        Some(Got::Found(_)) => Ok(()),
        Some(Got::Missing) => Err(PageStoreError::NotFound),
        _ => Err(PageStoreError::Unavailable),
    }
}

/// The names of the directories under `/pages` that parse as page names.
pub(crate) async fn pages(board: &Arc<Board>) -> Result<Vec<PageName>, PageStoreError> {
    let rows = board
        .ls("/pages")
        .await
        .or_else(unavailable)?
        .unwrap_or_default();
    Ok(rows
        .iter()
        .filter(|row| row.kind == "directory")
        .filter_map(|row| row.logical.strip_prefix("/pages/"))
        .filter_map(|name| PageName::try_from(name).ok())
        .collect())
}

/// Answers `Ok` when `made_from` names nothing or names a page version or a
/// binding that exists.
pub(crate) async fn check_origin(
    board: &Arc<Board>,
    made_from: &MadeFrom,
) -> Result<(), PageStoreError> {
    let found = match made_from {
        MadeFrom::Page(pin) => at(board, pin).await.map(|_| ()),
        MadeFrom::Binding(name) => bindings::read(board, name).await.map(|_| ()),
        _ => Ok(()),
    };
    match found {
        Err(PageStoreError::NotFound) => Err(PageStoreError::UnresolvedOrigin),
        other => other,
    }
}

/// The Git blob id of a file in the working tree, or `None` when it is
/// absent.
fn file_blob(got: Option<Got>) -> Result<Option<String>, PageStoreError> {
    match got {
        Some(Got::Found(entry)) if entry.location.pointers.is_empty() => Ok(Some(entry.blob)),
        Some(Got::Missing) => Ok(None),
        _ => Err(PageStoreError::Unavailable),
    }
}

fn expect(blob: Option<&String>) -> Value {
    blob.map_or_else(|| json!("none"), |blob| json!(blob))
}

/// Publishes `page` as a new version. Each attempt reads the page's files
/// and its newest version, then sends one batch whose every line expects
/// what it read. A stale answer means another publish won, so the store
/// reads again, at most `Limits::retries` more times.
pub(crate) async fn publish(
    board: &Arc<Board>,
    page: &Page,
    provenance: &Provenance,
) -> Result<PageVersion, PageStoreError> {
    check_origin(board, provenance.made_from()).await?;
    let name = page.name();
    let format = page.content().format();
    let other = match format {
        ContentFormat::Markdown => ContentFormat::Html,
        _ => ContentFormat::Markdown,
    };
    let record = record_path(name);
    let (content, stale_content) = (content_path(name, format), content_path(name, other));
    let schema = stored::schema_line(PAGE_TYPE).map_err(|_| PageStoreError::Unavailable)?;
    let attempts = board.limits.retries.saturating_add(1);
    for attempt in 1..=attempts {
        let got = board
            .get(&[record.as_str(), content.as_str(), stale_content.as_str()])
            .await
            .or_else(unavailable)?;
        let mut got = got.into_iter();
        let (record_blob, content_blob, stale_blob) = (
            file_blob(got.next())?,
            file_blob(got.next())?,
            file_blob(got.next())?,
        );
        let previous = match &record_blob {
            None => None,
            Some(blob) => {
                // A page whose last change the walk cannot reach, past
                // 100,000 commits or at a shallow boundary, answers
                // Unavailable, so no publish names a wrong previous version.
                let row = match newest(board, name).await {
                    Err(PageStoreError::NotFound) => return Err(PageStoreError::Unavailable),
                    other => other?,
                };
                // The record's bytes differ from its last commit's when
                // another publish committed between the two reads, or when
                // a stopped put left them uncommitted. A new read settles
                // the first; the last attempt replaces the second.
                if row.blob.as_ref() != Some(blob) && attempt < attempts {
                    continue;
                }
                Some(row.commit)
            }
        };
        let stored_record =
            stored::to_value(&PageRecord::of(page, provenance, previous.as_deref()))
                .map_err(|_| PageStoreError::Unavailable)?;
        let mut lines = vec![
            json!({"path": content, "expect": expect(content_blob.as_ref()), "text": page.content().text()}),
            json!({"path": record, "expect": expect(record_blob.as_ref()), "record": stored_record}),
            schema.clone(),
        ];
        if let Some(blob) = &stale_blob {
            lines.push(json!({"path": stale_content, "expect": blob, "remove": true}));
        }
        match board.put_batch(&lines).await {
            Ok(report) => {
                let commit = report.data.commit.ok_or(PageStoreError::Unavailable)?;
                return PageVersion::try_from(commit.as_str())
                    .map_err(|_| PageStoreError::Unavailable);
            }
            Err(Failure::Board(failure)) if is_stale(&failure) => {}
            Err(other) => return unavailable(other),
        }
    }
    Err(PageStoreError::Unavailable)
}

/// Whether every refusal of a batch is a stale expect.
pub(crate) fn is_stale(failure: &crate::report::BoardFailure) -> bool {
    let refusals = failure.refusals();
    !refusals.is_empty()
        && refusals
            .iter()
            .all(|refusal| refusal.stale_found().is_some())
}

//! Bindings: saving one, moving its pin, checking its fit and listing them.
//!
//! A binding is the `flare/binding/v1` record at `/bindings/<name>`. A save
//! puts it with `--expect none`, so of two saves of one name the second is
//! stale and answers `NameTaken`. A move puts it with the expect of the
//! blob it read, so of two moves from one pin the second is stale and reads
//! the pin the first one left.

use crate::pages::{self, is_stale};
use crate::process::Board;
use crate::report::{Failure, Got};
use crate::stored::{self, BINDING_TYPE, BindingRecord};
use flare_core::{
    Binding, BindingName, FillTypes, MoveError, PageName, PagePin, PageStoreError, PageVersion,
    RecordStore, WriteProvenance,
};
use serde_json::json;
use std::sync::Arc;

fn binding_path(name: &BindingName) -> String {
    format!("/bindings/{name}")
}

fn unavailable<T>(_failure: Failure) -> Result<T, PageStoreError> {
    Err(PageStoreError::Unavailable)
}

/// The binding stored under `name`, with the Git blob id of its file.
pub(crate) async fn read(
    board: &Arc<Board>,
    name: &BindingName,
) -> Result<(Binding, String), PageStoreError> {
    let path = binding_path(name);
    let got = board.get(&[path.as_str()]).await.or_else(unavailable)?;
    match got.into_iter().next() {
        Some(Got::Found(entry)) if entry.location.pointers.is_empty() => {
            let record = entry.record.ok_or(PageStoreError::Unavailable)?;
            let stored: BindingRecord =
                stored::from_value(&record).map_err(|_| PageStoreError::Unavailable)?;
            let binding = stored
                .binding(name)
                .map_err(|_| PageStoreError::Unavailable)?;
            Ok((binding, entry.blob))
        }
        Some(Got::Missing) => Err(PageStoreError::NotFound),
        _ => Err(PageStoreError::Unavailable),
    }
}

/// The page at `pin`, or `UnknownVersion` when the page has no such
/// version.
async fn pinned_page(
    board: &Arc<Board>,
    pin: &PagePin,
) -> Result<flare_core::Page, PageStoreError> {
    match pages::at(board, pin).await {
        Ok(published) => Ok(published.page().clone()),
        Err(PageStoreError::NotFound) => Err(PageStoreError::UnknownVersion),
        Err(other) => Err(other),
    }
}

/// The batch that writes `binding` with `expect`, and its schema.
fn lines(binding: &Binding, expect: &str) -> Result<[serde_json::Value; 2], PageStoreError> {
    let record =
        stored::to_value(&BindingRecord::of(binding)).map_err(|_| PageStoreError::Unavailable)?;
    let schema = stored::schema_line(BINDING_TYPE).map_err(|_| PageStoreError::Unavailable)?;
    Ok([
        json!({"path": binding_path(binding.name()), "expect": expect, "record": record}),
        schema,
    ])
}

pub(crate) async fn save(
    board: &Arc<Board>,
    binding: &Binding,
    records: &impl RecordStore,
) -> Result<(), PageStoreError> {
    match read(board, binding.name()).await {
        Ok(_) => return Err(PageStoreError::NameTaken),
        Err(PageStoreError::NotFound) => {}
        Err(other) => return Err(other),
    }
    let page = pinned_page(board, binding.pin()).await?;
    pages::check_origin(board, binding.provenance().made_from()).await?;
    FillTypes::read(binding.fills(), records)
        .await
        .check(&page)?;
    match board.put_batch(&lines(binding, "none")?).await {
        Ok(report) if report.data.commit.is_some() => Ok(()),
        Ok(_) => Err(PageStoreError::Unavailable),
        Err(Failure::Board(failure)) if is_stale(&failure) => Err(PageStoreError::NameTaken),
        Err(other) => unavailable(other),
    }
}

pub(crate) async fn check_fit(
    board: &Arc<Board>,
    name: &BindingName,
    records: &impl RecordStore,
) -> Result<(), PageStoreError> {
    let (binding, _) = read(board, name).await?;
    let page = match pages::at(board, binding.pin()).await {
        Ok(published) => published.page().clone(),
        Err(PageStoreError::NotFound) => return Err(PageStoreError::Unavailable),
        Err(other) => return Err(other),
    };
    FillTypes::read(binding.fills(), records).await.check(&page)
}

/// Moves the pin of `name` from `expected` to `version`. A stale answer
/// means another writer changed the binding, so the store reads it again:
/// a pin other than `expected` answers `Moved`, and an unchanged pin
/// retries, at most `Limits::retries` more times.
pub(crate) async fn move_pin(
    board: &Arc<Board>,
    name: &BindingName,
    expected: &PageVersion,
    version: &PageVersion,
    records: &impl RecordStore,
    mover: &WriteProvenance,
) -> Result<(), MoveError> {
    let attempts = board.limits.retries.saturating_add(1);
    for _ in 0..attempts {
        let (binding, blob) = read(board, name).await?;
        if binding.pin().version() != expected {
            return Err(MoveError::Moved {
                current: binding.pin().version().clone(),
            });
        }
        let target = PagePin::new(binding.pin().name().clone(), version.clone());
        let page = pinned_page(board, &target).await?;
        FillTypes::read(binding.fills(), records)
            .await
            .check(&page)?;
        let moved = Binding::new(
            binding.name().clone(),
            target,
            binding.fills().clone(),
            binding.provenance().clone(),
        )
        .with_last_move(mover.clone());
        match board.put_batch(&lines(&moved, &blob)?).await {
            Ok(report) if report.data.commit.is_some() => return Ok(()),
            Ok(_) => return Err(PageStoreError::Unavailable.into()),
            Err(Failure::Board(failure)) if is_stale(&failure) => {}
            Err(_) => return Err(PageStoreError::Unavailable.into()),
        }
    }
    Err(PageStoreError::Unavailable.into())
}

/// The bindings pinned to any version of the page `name`.
pub(crate) async fn of_page(
    board: &Arc<Board>,
    name: &PageName,
) -> Result<Vec<Binding>, PageStoreError> {
    pages::exists(board, name).await?;
    let rows = board
        .ls("/bindings")
        .await
        .or_else(unavailable)?
        .unwrap_or_default();
    let mut names = Vec::new();
    for row in rows.iter().filter(|row| row.kind == "record") {
        let Some(found) = row.logical.strip_prefix("/bindings/") else {
            continue;
        };
        if let Ok(binding) = BindingName::try_from(found) {
            names.push(binding);
        }
    }
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let paths: Vec<String> = names.iter().map(binding_path).collect();
    let texts: Vec<&str> = paths.iter().map(String::as_str).collect();
    let got = board.get(&texts).await.or_else(unavailable)?;
    let mut found = Vec::new();
    for (binding_name, answer) in names.iter().zip(got) {
        let Got::Found(entry) = answer else {
            continue;
        };
        let record = entry.record.ok_or(PageStoreError::Unavailable)?;
        let stored: BindingRecord =
            stored::from_value(&record).map_err(|_| PageStoreError::Unavailable)?;
        let binding = stored
            .binding(binding_name)
            .map_err(|_| PageStoreError::Unavailable)?;
        if binding.pin().name() == name {
            found.push(binding);
        }
    }
    Ok(found)
}

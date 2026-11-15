//! An edit applied to the record as the server reads it now.

use crate::body;
use crate::post::{Edit, EditField, newlines};
use hangar_core::{BindingName, Blob, Principal, Record, RecordBody, RecordPath, WriteProvenance};
use serde_json::Value;

/// What applying an edit gives.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Outcome {
    /// Write this body.
    Write(RecordWrite),
    /// Nothing to write: the person changed no field, or the record already
    /// holds every change.
    Unchanged,
    /// The record changed since the form was drawn, and it does not hold the
    /// person's changes. No write was built.
    Conflict(Conflict),
}

/// The write that an edit makes: the record's path, its new body, the blob
/// the edit was based on and the binding it went through.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RecordWrite {
    path: RecordPath,
    body: RecordBody,
    based_on: Blob,
    through: BindingName,
}

impl RecordWrite {
    /// The record to write.
    #[must_use]
    pub fn path(&self) -> &RecordPath {
        &self.path
    }

    /// The new body, as pretty-printed JSON.
    #[must_use]
    pub fn body(&self) -> &RecordBody {
        &self.body
    }

    /// The blob the edit was based on.
    #[must_use]
    pub fn based_on(&self) -> &Blob {
        &self.based_on
    }

    /// The binding the edit went through.
    #[must_use]
    pub fn through(&self) -> &BindingName {
        &self.through
    }

    /// The provenance that `RecordStore::write` takes for this write, made by
    /// `made_by`.
    #[must_use]
    pub fn provenance(&self, made_by: Principal) -> WriteProvenance {
        WriteProvenance::new(self.based_on.clone(), made_by, self.through.clone())
    }
}

/// An edit that met a newer record: for each drawn field, the value now, the
/// value when the form was drawn and the person's entry.
#[derive(Debug, Clone)]
pub struct Conflict {
    pub(crate) edit: Edit,
    pub(crate) path: RecordPath,
    pub(crate) newer: Blob,
    pub(crate) now: Vec<Option<Value>>,
}

impl Conflict {
    /// The record the edit is for.
    #[must_use]
    pub fn path(&self) -> &RecordPath {
        &self.path
    }

    /// The blob of the newer record.
    #[must_use]
    pub fn newer(&self) -> &Blob {
        &self.newer
    }
}

/// Why an edit could not be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ApplyError {
    /// The record's body is not one JSON object.
    #[error("the record {path} is not a JSON object")]
    NotAnObject {
        /// The record.
        path: RecordPath,
    },
    /// The record's type is not the type of the slot the edit is for.
    #[error("the record {path} is not of the slot's type")]
    WrongType {
        /// The record.
        path: RecordPath,
    },
    /// The new body is larger than a record may be.
    #[error("the edited record would be larger than a record may be")]
    TooLarge,
}

impl Edit {
    /// Applies the edit to `record`, as the server reads it now.
    ///
    /// When the record's blob equals the edit's `based_on`, each changed
    /// field is set, an optional field that was emptied is removed, every
    /// other field stays as it is, and the result is
    /// [`Outcome::Write`]. With no changed field it is
    /// [`Outcome::Unchanged`]. When the blob differs, the result is
    /// [`Outcome::Unchanged`] if the record already holds every change, and
    /// [`Outcome::Conflict`] otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`ApplyError::NotAnObject`] for a body that is not one JSON
    /// object, [`ApplyError::WrongType`] for a record of another type, and
    /// [`ApplyError::TooLarge`] when the new body is over 1 MiB.
    pub fn apply(&self, record: &Record) -> Result<Outcome, ApplyError> {
        if record.record_type() != &self.record_type {
            return Err(ApplyError::WrongType {
                path: record.path().clone(),
            });
        }
        let mut fields = body::fields(record.body()).ok_or_else(|| ApplyError::NotAnObject {
            path: record.path().clone(),
        })?;
        let changed: Vec<&EditField> = self
            .fields
            .iter()
            .filter(|field| !same(field.entered.as_ref(), field.base.as_ref()))
            .collect();
        if changed.is_empty() {
            return Ok(Outcome::Unchanged);
        }
        if record.blob() != &self.based_on {
            let held = changed
                .iter()
                .all(|field| same(field.entered.as_ref(), fields.get(field.name.as_str())));
            if held {
                return Ok(Outcome::Unchanged);
            }
            return Ok(Outcome::Conflict(Conflict {
                now: self
                    .fields
                    .iter()
                    .map(|field| fields.get(field.name.as_str()).cloned())
                    .collect(),
                edit: self.clone(),
                path: record.path().clone(),
                newer: record.blob().clone(),
            }));
        }
        for field in changed {
            match &field.entered {
                Some(value) => fields.insert(field.name.as_str().to_owned(), value.clone()),
                None => fields.remove(field.name.as_str()),
            };
        }
        let text = format!("{:#}", Value::Object(fields));
        let body = RecordBody::try_from(text.as_str()).map_err(|_| ApplyError::TooLarge)?;
        Ok(Outcome::Write(RecordWrite {
            path: record.path().clone(),
            body,
            based_on: record.blob().clone(),
            through: self.through.clone(),
        }))
    }
}

/// Whether two values of a field are the same edit: no value, null, an empty
/// string and `false` all count as empty, strings compare after their line
/// endings are made LF, and numbers compare by value.
pub(crate) fn same(left: Option<&Value>, right: Option<&Value>) -> bool {
    let empty = |value: Option<&Value>| match value {
        None | Some(Value::Null | Value::Bool(false)) => true,
        Some(Value::String(text)) => text.is_empty(),
        Some(_) => false,
    };
    match (left, right) {
        _ if empty(left) || empty(right) => empty(left) && empty(right),
        (Some(Value::String(left)), Some(Value::String(right))) => {
            newlines(left) == newlines(right)
        }
        (Some(Value::Number(left)), Some(Value::Number(right))) => {
            left == right
                || left
                    .as_f64()
                    .is_some_and(|left| Some(left) == right.as_f64())
        }
        _ => left == right,
    }
}

#[cfg(test)]
mod tests;

//! The post of an edit form, read and checked against its slot and schema.

mod entry;

pub(crate) use entry::newlines;

use crate::body::Fields;
use crate::form::{MAX_FIELDS, MAX_VALUE_BYTES};
use crate::schema::{FieldKind, Schema};
use crate::token::FormToken;
use crate::values;
use hangar_core::{BindingName, Blob, FieldName, ParseError, RecordType, Slot, SlotName};
use serde_json::Value;
use std::collections::BTreeMap;

/// The hidden inputs every form carries, in the order a missing one is
/// reported.
const HIDDEN: [&str; 5] = ["token", "based_on", "through", "slot", "base"];
/// The most pairs a form's post holds: the hidden inputs and one per field.
const MAX_PAIRS: usize = HIDDEN.len() + MAX_FIELDS;
/// The largest `base` in bytes: the server's 1 MiB request body limit.
const MAX_BASE_BYTES: usize = 1024 * 1024;

/// A form post read from its decoded name and value pairs. Its `token`,
/// `based_on`, `through` and `slot` are the browser's claims: the server
/// compares them with what the token is tied to before it calls
/// [`FormPost::into_edit`].
#[derive(Debug, Clone)]
pub struct FormPost {
    token: FormToken,
    based_on: Blob,
    through: BindingName,
    slot: SlotName,
    base: Fields,
    entries: BTreeMap<FieldName, String>,
}

/// Why a post was refused. Each leaves the record unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PostError {
    /// The post lacks one of the form's hidden inputs.
    #[error("the post has no {key}")]
    Missing {
        /// The missing key.
        key: &'static str,
    },
    /// The post has no entry for a field the form drew, other than a
    /// checkbox, which a browser leaves out when it is unchecked.
    #[error("the post has no entry for the field {field}")]
    MissingField {
        /// The field.
        field: FieldName,
    },
    /// The post names a key twice.
    #[error("the post gives {key} twice")]
    Repeated {
        /// The repeated key.
        key: String,
    },
    /// The post holds a key that no form draws.
    #[error("the post holds the unknown key {key}")]
    Unknown {
        /// The unknown key.
        key: String,
    },
    /// A hidden input's value is malformed.
    #[error("the post's {key} is malformed")]
    Malformed {
        /// The key whose value is malformed.
        key: &'static str,
    },
    /// A name in the post is not a valid field, slot or binding name.
    #[error("the post holds a malformed name")]
    Name(#[source] ParseError),
    /// The post is larger than any form this crate draws.
    #[error("the post is larger than any form hangar draws")]
    TooLarge,
    /// The post names another slot.
    #[error("the post is for slot {found}, not {expected}")]
    WrongSlot {
        /// The slot the edit is for.
        expected: SlotName,
        /// The slot the post names.
        found: SlotName,
    },
    /// The post edits a field the form did not draw.
    #[error("the form did not draw the field {field}")]
    NotDrawn {
        /// The field.
        field: FieldName,
    },
    /// The post edits a field the slot does not list as editable.
    #[error("the field {field} is not editable on this page")]
    NotEditable {
        /// The field.
        field: FieldName,
    },
    /// The post edits a field whose schema shape no form edits.
    #[error("the field {field} cannot be edited here")]
    Unsupported {
        /// The field.
        field: FieldName,
    },
    /// A field's entry does not parse to the field's kind.
    #[error("the field {label} needs {expected}")]
    BadValue {
        /// The field.
        field: FieldName,
        /// The field's label.
        label: String,
        /// What the field takes, such as "a whole number".
        expected: &'static str,
    },
}

impl FormPost {
    /// Reads a post from its decoded name and value pairs.
    ///
    /// # Errors
    ///
    /// Returns [`PostError::Missing`], [`PostError::Repeated`] or
    /// [`PostError::Unknown`] for a missing, repeated or unknown key,
    /// [`PostError::Malformed`] or [`PostError::Name`] for a malformed blob,
    /// name or `base`, and [`PostError::TooLarge`] for a post larger than any
    /// form draws.
    pub fn from_pairs(
        pairs: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, PostError> {
        let mut hidden: [Option<String>; 5] = Default::default();
        let mut entries = BTreeMap::new();
        for (count, (key, value)) in pairs.into_iter().enumerate() {
            let limit = if key == "base" {
                MAX_BASE_BYTES
            } else {
                MAX_VALUE_BYTES
            };
            // A browser posts each line break as CRLF, and the form drew the
            // value with LF line endings, so the limit counts a CRLF as one
            // byte.
            let length = value.len().saturating_sub(value.matches("\r\n").count());
            if count >= MAX_PAIRS || length > limit {
                return Err(PostError::TooLarge);
            }
            if let Some(index) = HIDDEN.iter().position(|name| *name == key) {
                let slot = hidden
                    .get_mut(index)
                    .ok_or(PostError::Unknown { key: key.clone() })?;
                if slot.replace(value).is_some() {
                    return Err(PostError::Repeated { key });
                }
                continue;
            }
            let Some(name) = key.strip_prefix("f.") else {
                return Err(PostError::Unknown { key });
            };
            let field = FieldName::try_from(name).map_err(PostError::Name)?;
            if entries.insert(field, value).is_some() {
                return Err(PostError::Repeated { key });
            }
        }
        let [token, based_on, through, slot, base] = hidden;
        let present =
            |value: Option<String>, key: &'static str| value.ok_or(PostError::Missing { key });
        let token = FormToken::new(present(token, "token")?);
        let based_on = present(based_on, "based_on")?;
        let through = present(through, "through")?;
        let slot = present(slot, "slot")?;
        let base = present(base, "base")?;
        if token.is_empty() {
            return Err(PostError::Malformed { key: "token" });
        }
        let based_on = Blob::try_from(based_on.as_str())
            .map_err(|_| PostError::Malformed { key: "based_on" })?;
        let through = BindingName::try_from(through.as_str()).map_err(PostError::Name)?;
        let slot = SlotName::try_from(slot.as_str()).map_err(PostError::Name)?;
        let Ok(Value::Object(base)) = serde_json::from_str::<Value>(&base) else {
            return Err(PostError::Malformed { key: "base" });
        };
        if base.len() > MAX_FIELDS {
            return Err(PostError::TooLarge);
        }
        for key in base.keys() {
            FieldName::try_from(key.as_str()).map_err(PostError::Name)?;
        }
        Ok(Self {
            token,
            based_on,
            through,
            slot,
            base,
            entries,
        })
    }

    /// The token the post carries.
    #[must_use]
    pub fn token(&self) -> &FormToken {
        &self.token
    }

    /// The blob the post claims its form was drawn from.
    #[must_use]
    pub fn based_on(&self) -> &Blob {
        &self.based_on
    }

    /// The binding the post claims to go through.
    #[must_use]
    pub fn through(&self) -> &BindingName {
        &self.through
    }

    /// The slot the post claims to edit.
    #[must_use]
    pub fn slot(&self) -> &SlotName {
        &self.slot
    }

    /// The entry for `field`, as the browser sent it.
    pub(crate) fn entry(&self, field: &FieldName) -> Option<&str> {
        self.entries.get(field).map(String::as_str)
    }

    /// The loaded values of the fields the form drew.
    pub(crate) fn base(&self) -> &Fields {
        &self.base
    }

    /// Checks the post against the slot and schema of the record it edits.
    ///
    /// # Errors
    ///
    /// Returns [`PostError::WrongSlot`] for a post that names another slot,
    /// [`PostError::NotDrawn`], [`PostError::NotEditable`] or
    /// [`PostError::Unsupported`] for a field the form did not draw, and
    /// [`PostError::BadValue`] for an entry that does not parse to its
    /// field's kind.
    pub fn into_edit(self, slot: &Slot, schema: &Schema) -> Result<Edit, PostError> {
        if &self.slot != slot.name() {
            return Err(PostError::WrongSlot {
                expected: slot.name().clone(),
                found: self.slot,
            });
        }
        if let Some(field) = self
            .entries
            .keys()
            .find(|field| !self.base.contains_key(field.as_str()))
        {
            return Err(PostError::NotDrawn {
                field: field.clone(),
            });
        }
        for key in self.base.keys() {
            let field = FieldName::try_from(key.as_str()).map_err(PostError::Name)?;
            if !slot.editable_fields().contains(&field) {
                return Err(PostError::NotEditable { field });
            }
            if schema.field(&field) == FieldKind::Unsupported {
                return Err(PostError::Unsupported { field });
            }
        }
        let mut fields = Vec::new();
        for field in slot.editable_fields() {
            let Some(base) = self.base.get(field.as_str()) else {
                continue;
            };
            if fields.iter().any(|drawn: &EditField| &drawn.name == field) {
                continue;
            }
            let kind = schema.field(field);
            let label = values::label(schema, field);
            let required = schema.spec(field).is_some_and(|spec| spec.required);
            let entered = entry::read(
                &kind,
                self.entries.get(field).map(String::as_str),
                required,
                base,
            )
            .map_err(|problem| match problem {
                entry::Problem::Missing => PostError::MissingField {
                    field: field.clone(),
                },
                entry::Problem::Bad(expected) => PostError::BadValue {
                    field: field.clone(),
                    label: label.clone(),
                    expected,
                },
            })?;
            fields.push(EditField {
                name: field.clone(),
                kind,
                label,
                base: Some(base.clone()).filter(|value| !value.is_null()),
                entered,
            });
        }
        Ok(Edit {
            record_type: slot.record_type().clone(),
            noun: values::noun(slot.name()),
            slot: self.slot,
            based_on: self.based_on,
            through: self.through,
            fields,
        })
    }
}

/// A checked edit of one record: for each drawn field, its value when the
/// form was drawn and the value the person entered.
#[derive(Debug, Clone)]
pub struct Edit {
    pub(crate) record_type: RecordType,
    pub(crate) noun: String,
    pub(crate) slot: SlotName,
    pub(crate) based_on: Blob,
    pub(crate) through: BindingName,
    pub(crate) fields: Vec<EditField>,
}

/// One drawn field of an edit.
#[derive(Debug, Clone)]
pub(crate) struct EditField {
    pub(crate) name: FieldName,
    pub(crate) kind: FieldKind,
    pub(crate) label: String,
    pub(crate) base: Option<Value>,
    pub(crate) entered: Option<Value>,
}

#[cfg(test)]
mod tests;

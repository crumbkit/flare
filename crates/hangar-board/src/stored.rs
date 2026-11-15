//! The records hangar-board stores, their embedded schemas, and their
//! conversion to and from hangar-core's types.
//!
//! Each schema travels as a schema line in the same batch as the records of
//! its type, so a board needs no install step. A stored value that no
//! longer parses as hangar-core's type answers [`Corrupt`], which the
//! stores answer as `Unavailable`.

use hangar_core::{
    Binding, BindingName, Blob, ContentFormat, FieldName, Fills, MadeFrom, Page, PageContent,
    PageName, PagePin, PageVersion, Principal, PrincipalId, Provenance, RecordBody, RecordPath,
    RecordRef, RecordType, Sample, Slot, SlotName, Slots, WriteProvenance,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The type of a page version's record.
pub(crate) const PAGE_TYPE: &str = "hangar/page/v1";
/// The type of a binding's record.
pub(crate) const BINDING_TYPE: &str = "hangar/binding/v1";
/// The type of a record edit's record.
pub(crate) const EDIT_TYPE: &str = "hangar/edit/v1";

const PAGE_SCHEMA: &str = include_str!("stored/page.schema.json");
const BINDING_SCHEMA: &str = include_str!("stored/binding.schema.json");
const EDIT_SCHEMA: &str = include_str!("stored/edit.schema.json");

/// A stored record that hangar-core's types refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Corrupt;

/// The batch line that adds the schema of `record_type`, one of this
/// module's three types.
pub(crate) fn schema_line(record_type: &str) -> Result<Value, Corrupt> {
    let text = match record_type {
        PAGE_TYPE => PAGE_SCHEMA,
        BINDING_TYPE => BINDING_SCHEMA,
        EDIT_TYPE => EDIT_SCHEMA,
        _ => return Err(Corrupt),
    };
    let schema: Value = serde_json::from_str(text).map_err(|_| Corrupt)?;
    Ok(json!({"path": format!("/.board/types/{record_type}.schema"), "record": schema}))
}

/// What a page or a binding was made from, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum Origin {
    Original,
    Page { page: String, version: String },
    Binding { binding: String },
}

/// The three parts of a [`Provenance`], as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Made {
    made_by: String,
    made_from: Origin,
    sources: Vec<String>,
}

impl Made {
    fn of(provenance: &Provenance) -> Self {
        let made_from = match provenance.made_from() {
            MadeFrom::Page(pin) => Origin::Page {
                page: pin.name().to_string(),
                version: pin.version().to_string(),
            },
            MadeFrom::Binding(name) => Origin::Binding {
                binding: name.to_string(),
            },
            _ => Origin::Original,
        };
        Self {
            made_by: provenance.made_by().id().to_string(),
            made_from,
            sources: provenance
                .sources()
                .iter()
                .map(|source| source.path().to_string())
                .collect(),
        }
    }

    fn provenance(&self) -> Result<Provenance, Corrupt> {
        let made_from = match &self.made_from {
            Origin::Original => MadeFrom::Original,
            Origin::Page { page, version } => MadeFrom::Page(PagePin::new(
                parse(PageName::try_from(page.as_str()))?,
                parse(PageVersion::try_from(version.as_str()))?,
            )),
            Origin::Binding { binding } => {
                MadeFrom::Binding(parse(BindingName::try_from(binding.as_str()))?)
            }
        };
        let mut sources = Vec::new();
        for source in &self.sources {
            sources.push(RecordRef::new(parse(RecordPath::try_from(
                source.as_str(),
            ))?));
        }
        parse(Provenance::new(
            principal(&self.made_by)?,
            made_from,
            sources,
        ))
    }
}

fn parse<T, E>(parsed: Result<T, E>) -> Result<T, Corrupt> {
    parsed.map_err(|_| Corrupt)
}

fn principal(id: &str) -> Result<Principal, Corrupt> {
    Ok(Principal::new(parse(PrincipalId::try_from(id))?))
}

/// One slot of a page version, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSlot {
    name: String,
    #[serde(rename = "type")]
    record_type: String,
    editable_fields: Vec<String>,
    sample: Option<StoredSample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredSample {
    #[serde(rename = "type")]
    record_type: String,
    body: String,
}

/// `/pages/<name>/page.json`, a `hangar/page/v1` record.
///
/// serde does not combine `deny_unknown_fields` with `flatten`, so the
/// embedded schema's `additionalProperties: false` refuses unknown fields
/// when board writes the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PageRecord {
    #[serde(rename = "$type")]
    record_type: String,
    format: String,
    slots: Vec<StoredSlot>,
    #[serde(flatten)]
    made: Made,
    previous: Option<String>,
}

impl PageRecord {
    /// The record of `page`, made as `provenance` says, that replaces the
    /// version `previous`.
    pub(crate) fn of(page: &Page, provenance: &Provenance, previous: Option<&str>) -> Self {
        let slots = page
            .slots()
            .iter()
            .map(|slot| StoredSlot {
                name: slot.name().to_string(),
                record_type: slot.record_type().to_string(),
                editable_fields: slot
                    .editable_fields()
                    .iter()
                    .map(|field| field.as_str().to_owned())
                    .collect(),
                sample: slot.sample().map(|sample| StoredSample {
                    record_type: sample.record_type().to_string(),
                    body: sample.body().as_str().to_owned(),
                }),
            })
            .collect();
        Self {
            record_type: PAGE_TYPE.to_owned(),
            format: format_name(page.content().format()).to_owned(),
            slots,
            made: Made::of(provenance),
            previous: previous.map(str::to_owned),
        }
    }

    /// The markup language of the version's text.
    pub(crate) fn format(&self) -> Result<ContentFormat, Corrupt> {
        match self.format.as_str() {
            "html" => Ok(ContentFormat::Html),
            "markdown" => Ok(ContentFormat::Markdown),
            _ => Err(Corrupt),
        }
    }

    /// The page named `name` with this record's slots and `text`, and the
    /// provenance of the version.
    pub(crate) fn page(&self, name: &PageName, text: &str) -> Result<(Page, Provenance), Corrupt> {
        let mut slots = Vec::new();
        for stored in &self.slots {
            let mut fields = Vec::new();
            for field in &stored.editable_fields {
                fields.push(parse(FieldName::try_from(field.as_str()))?);
            }
            let sample = match &stored.sample {
                Some(sample) => Some(Sample::new(
                    parse(RecordType::try_from(sample.record_type.as_str()))?,
                    parse(RecordBody::try_from(sample.body.as_str()))?,
                )),
                None => None,
            };
            slots.push(parse(Slot::new(
                parse(SlotName::try_from(stored.name.as_str()))?,
                parse(RecordType::try_from(stored.record_type.as_str()))?,
                fields,
                sample,
            ))?);
        }
        let content = parse(PageContent::new(self.format()?, text))?;
        let page = Page::new(name.clone(), content, parse(Slots::new(slots))?);
        Ok((page, self.made.provenance()?))
    }
}

/// The file name of a page's text in `format`.
pub(crate) fn content_file(format: ContentFormat) -> &'static str {
    match format {
        ContentFormat::Markdown => "page.md",
        _ => "page.html",
    }
}

fn format_name(format: ContentFormat) -> &'static str {
    match format {
        ContentFormat::Markdown => "markdown",
        _ => "html",
    }
}

/// The provenance of a move, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredMove {
    based_on: String,
    made_by: String,
    through: String,
}

/// `/bindings/<name>.json`, a `hangar/binding/v1` record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BindingRecord {
    #[serde(rename = "$type")]
    record_type: String,
    page: String,
    version: String,
    fills: BTreeMap<String, String>,
    #[serde(flatten)]
    made: Made,
    last_move: Option<StoredMove>,
}

impl BindingRecord {
    pub(crate) fn of(binding: &Binding) -> Self {
        Self {
            record_type: BINDING_TYPE.to_owned(),
            page: binding.pin().name().to_string(),
            version: binding.pin().version().to_string(),
            fills: binding
                .fills()
                .iter()
                .map(|(slot, path)| (slot.to_string(), path.to_string()))
                .collect(),
            made: Made::of(binding.provenance()),
            last_move: binding.last_move().map(|mover| StoredMove {
                based_on: mover.based_on().to_string(),
                made_by: mover.made_by().id().to_string(),
                through: mover.through().to_string(),
            }),
        }
    }

    /// The binding named `name` that this record holds.
    pub(crate) fn binding(&self, name: &BindingName) -> Result<Binding, Corrupt> {
        let mut fills = Vec::new();
        for (slot, path) in &self.fills {
            fills.push((
                parse(SlotName::try_from(slot.as_str()))?,
                parse(RecordPath::try_from(path.as_str()))?,
            ));
        }
        let pin = PagePin::new(
            parse(PageName::try_from(self.page.as_str()))?,
            parse(PageVersion::try_from(self.version.as_str()))?,
        );
        let binding = Binding::new(
            name.clone(),
            pin,
            parse(Fills::new(fills))?,
            self.made.provenance()?,
        );
        match &self.last_move {
            None => Ok(binding),
            Some(stored) => Ok(binding.with_last_move(WriteProvenance::new(
                parse(Blob::try_from(stored.based_on.as_str()))?,
                principal(&stored.made_by)?,
                parse(BindingName::try_from(stored.through.as_str()))?,
            ))),
        }
    }
}

/// A `hangar/edit/v1` record of one record edit.
pub(crate) fn edit_record(path: &RecordPath, provenance: &WriteProvenance) -> Value {
    json!({
        "$type": EDIT_TYPE,
        "record": path.as_str(),
        "based_on": provenance.based_on().as_str(),
        "made_by": provenance.made_by().id().as_str(),
        "through": provenance.through().as_str(),
    })
}

/// A record value as JSON, or [`Corrupt`] when it does not parse.
pub(crate) fn from_value<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, Corrupt> {
    T::deserialize(value).map_err(|_| Corrupt)
}

/// A record as the JSON value of a batch line.
pub(crate) fn to_value<T: Serialize>(record: &T) -> Result<Value, Corrupt> {
    serde_json::to_value(record).map_err(|_| Corrupt)
}

#[cfg(test)]
mod tests;

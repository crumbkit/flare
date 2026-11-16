//! `hangar publish`: reads a page file and its slots file and publishes a
//! new version through `hangar_server::ops::publish`.

use crate::board;
use crate::cli::PublishArgs;
use crate::codes::Failed;
use crate::documents::Published;
use crate::env::Snapshot;
use crate::pages;
use hangar_auth_loopback::local_principal;
use hangar_core::{
    ContentFormat, FieldName, MadeFrom, Page, PageContent, PageName, Provenance, RecordBody,
    RecordType, Sample, Slot, SlotName, Slots,
};
use hangar_server::ops::{self, PublishError};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

/// One slot in a slots file, in the form the stored page record uses, with
/// the sample's body as JSON.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SlotEntry {
    name: String,
    #[serde(rename = "type")]
    record_type: String,
    #[serde(default)]
    editable_fields: Vec<String>,
    sample: Option<SampleEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SampleEntry {
    #[serde(rename = "type")]
    record_type: String,
    body: Value,
}

/// Runs `hangar publish`. A dry run checks the page and the board and
/// writes nothing.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn run(
    args: &PublishArgs,
    dry_run: bool,
    env: &Snapshot,
) -> Result<Published, Failed> {
    let page = read_page(args)?;
    let (_, store) = board::checked_store(&args.board, env).await?;
    let report = |version| Published {
        name: page.name().as_str().to_owned(),
        version,
        format: match page.content().format() {
            ContentFormat::Markdown => "markdown",
            _ => "html",
        },
        slots: page
            .slots()
            .iter()
            .map(|slot| slot.name().to_string())
            .collect(),
    };
    if dry_run {
        hangar_render::check_page(&page).map_err(flaws)?;
        return Ok(report(None));
    }
    let made_by = local_principal(&env.user()).map_err(invalid)?;
    let provenance = Provenance::new(made_by, MadeFrom::Original, Vec::new()).map_err(invalid)?;
    let published = report(None);
    let version = ops::publish(&store, page, provenance)
        .await
        .map_err(|error| match error {
            PublishError::Flaws(found) => flaws(found),
            PublishError::Store(error) => pages::store_failure_text(error),
            other => pages::store_failure_text(other),
        })?;
    Ok(Published {
        version: Some(version.as_str().to_owned()),
        ..published
    })
}

fn flaws(found: hangar_render::TemplateFlaws) -> Failed {
    Failed::TemplateFlaws {
        flaws: found.to_string(),
    }
}

fn invalid(error: impl std::fmt::Display) -> Failed {
    Failed::PageInvalid {
        problem: error.to_string(),
    }
}

/// The page that `args` names, read from its files.
fn read_page(args: &PublishArgs) -> Result<Page, Failed> {
    let name = PageName::try_from(args.name.as_str()).map_err(invalid)?;
    let format = match args.file.extension().and_then(|ending| ending.to_str()) {
        Some("html") => ContentFormat::Html,
        Some("md") => ContentFormat::Markdown,
        _ => {
            return Err(Failed::UnknownFormat {
                path: args.file.display().to_string(),
            });
        }
    };
    let text = read(&args.file)?;
    let content = PageContent::new(format, text).map_err(invalid)?;
    let slots = match &args.slots {
        Some(path) => slots(&read(path)?)?,
        None => Slots::new(Vec::new()).map_err(invalid)?,
    };
    Ok(Page::new(name, content, slots))
}

fn read(path: &Path) -> Result<String, Failed> {
    std::fs::read_to_string(path).map_err(|error| Failed::PageUnreadable {
        path: path.display().to_string(),
        problem: error.to_string(),
    })
}

/// The slots in a slots file's text.
fn slots(text: &str) -> Result<Slots, Failed> {
    let entries: Vec<SlotEntry> =
        serde_json::from_str(text).map_err(|error| invalid(format!("the slots file: {error}")))?;
    let mut slots = Vec::new();
    for entry in entries {
        let mut fields = Vec::new();
        for field in &entry.editable_fields {
            fields.push(FieldName::try_from(field.as_str()).map_err(invalid)?);
        }
        let sample = match entry.sample {
            Some(sample) => Some(Sample::new(
                RecordType::try_from(sample.record_type.as_str()).map_err(invalid)?,
                RecordBody::try_from(sample.body.to_string().as_str()).map_err(invalid)?,
            )),
            None => None,
        };
        slots.push(
            Slot::new(
                SlotName::try_from(entry.name.as_str()).map_err(invalid)?,
                RecordType::try_from(entry.record_type.as_str()).map_err(invalid)?,
                fields,
                sample,
            )
            .map_err(invalid)?,
        );
    }
    Slots::new(slots).map_err(invalid)
}

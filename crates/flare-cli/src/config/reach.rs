//! The `reach` of an admit rule: `"everything"` or a list of the pages,
//! bindings and records it may touch.

use flare_auth_oidc::Reach;
use flare_core::{BindingName, PageName, RecordPath, Resource};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum ReachSection {
    Word(String),
    Only(Vec<ResourceSection>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceSection {
    page: Option<String>,
    binding: Option<String>,
    record: Option<String>,
}

pub(super) fn reach(section: ReachSection) -> Result<Reach, String> {
    match section {
        ReachSection::Word(word) if word == "everything" => Ok(Reach::Everything),
        ReachSection::Word(_) => Err("reach must be \"everything\" or a list".to_owned()),
        ReachSection::Only(items) => items
            .into_iter()
            .map(resource)
            .collect::<Result<Vec<_>, _>>()
            .map(Reach::Only),
    }
}

fn resource(section: ResourceSection) -> Result<Resource, String> {
    let bad = |error: flare_core::ParseError| format!("a reach entry is not valid: {error}");
    match (section.page, section.binding, section.record) {
        (Some(page), None, None) => PageName::try_from(page.as_str())
            .map(Resource::Page)
            .map_err(bad),
        (None, Some(binding), None) => BindingName::try_from(binding.as_str())
            .map(Resource::Binding)
            .map_err(bad),
        (None, None, Some(record)) => RecordPath::try_from(record.as_str())
            .map(Resource::Record)
            .map_err(bad),
        _ => Err("each reach entry names exactly one page, binding or record".to_owned()),
    }
}

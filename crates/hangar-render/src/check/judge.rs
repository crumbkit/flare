//! Refuses each Mustache tag that sits where a record value could change
//! the markup.

use super::{FlawKind, Found, Place, Site, TagKind};

/// Attributes whose value is a URL or a style, which a record value may not
/// choose (threat 9). SVG animation's `to`, `from`, `values`, `by` and
/// `attributeName` are listed too, because `<set attributeName="href">` sets
/// a link's target from them.
const URL_OR_STYLE: &[&str] = &[
    "href",
    "src",
    "action",
    "formaction",
    "srcset",
    "ping",
    "poster",
    "data",
    "cite",
    "background",
    "xlink:href",
    "style",
    "to",
    "from",
    "values",
    "by",
    "attributename",
];

/// The flaw of each tag that sits where a record value could change the
/// markup.
pub(super) fn judge(sites: &[Site]) -> Vec<Found> {
    sites
        .iter()
        .filter_map(|site| judge_one(site).map(|kind| Found { kind, at: site.at }))
        .collect()
}

fn judge_one(site: &Site) -> Option<FlawKind> {
    let section = matches!(
        site.tag,
        TagKind::Section | TagKind::Inverted | TagKind::Close
    );
    match (&site.place, site.tag) {
        (_, TagKind::Refused) | (Place::Text, _) => None,
        (Place::AfterLessThan, _) => Some(FlawKind::TagAfterLessThan),
        (Place::Comment, _) => Some(FlawKind::TagInComment),
        (Place::RawText, _) => Some(FlawKind::TagInRawText),
        (Place::DoubleQuoted { .. } | Place::SingleQuoted | Place::Unquoted | Place::Markup, _)
            if section =>
        {
            Some(FlawKind::SectionInMarkup)
        }
        (Place::DoubleQuoted { end_tag: true, .. } | Place::Markup, _) => {
            Some(FlawKind::TagInMarkup)
        }
        (Place::DoubleQuoted { .. }, TagKind::Comment) => None,
        (Place::DoubleQuoted { attribute, .. }, _) => URL_OR_STYLE
            .contains(&attribute.as_str())
            .then(|| FlawKind::TagInUrlOrStyle {
                attribute: attribute.clone(),
            }),
        (Place::SingleQuoted, _) => Some(FlawKind::TagInSingleQuotedValue),
        (Place::Unquoted, _) => Some(FlawKind::TagInUnquotedValue),
    }
}

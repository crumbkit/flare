//! flare's filters: the list that `GET /api/search/fields` answers, and the
//! check of `type:` values against the kinds of share flare serves.

use crate::error::{ErrorCode, QueryError, message};
use crate::query::{Field, Query, Term, Value};

/// What a field's values are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// One of a fixed list of words.
    Keyword,
    /// A person: `me`, an email address or a name the deployment knows.
    Person,
}

/// One of flare's filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldInfo {
    /// The field.
    pub field: Field,
    /// What its values are.
    pub kind: FieldKind,
    /// The values a builder offers.
    pub values: &'static [&'static str],
    /// What it matches, in one sentence.
    pub about: &'static str,
}

/// flare's filters, in the order a builder offers them.
pub const FIELDS: &[FieldInfo] = &[
    FieldInfo {
        field: Field::Type,
        kind: FieldKind::Keyword,
        values: &["markdown", "html"],
        about: "The kind of share: markdown or html.",
    },
    FieldInfo {
        field: Field::From,
        kind: FieldKind::Person,
        values: &["me"],
        about: "The person who shared it: me, an email address or a name this server knows.",
    },
];

/// A kind of share that `type:` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareType {
    /// A Markdown share.
    Markdown,
    /// An HTML share. flare serves no HTML shares yet, so it matches none.
    Html,
}

impl ShareType {
    /// The kind that `value` names, in any case.
    #[must_use]
    pub fn named(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("markdown") {
            Some(Self::Markdown)
        } else if value.eq_ignore_ascii_case("html") {
            Some(Self::Html)
        } else {
            None
        }
    }
}

/// The error for a `from:` value that names nobody the deployment knows.
#[must_use]
pub fn unknown_person(value: &Value) -> QueryError {
    message(ErrorCode::UnknownPerson, value.span, None, &value.text)
}

/// Every `type:` value of `query` that names no kind of share, in the
/// order of the text.
#[must_use]
pub fn check(query: &Query) -> Vec<QueryError> {
    query
        .parts()
        .iter()
        .filter_map(|part| match &part.term {
            Term::Filter {
                field: Field::Type,
                values,
            } => Some(values),
            _ => None,
        })
        .flatten()
        .filter(|value| ShareType::named(&value.text).is_none())
        .map(|value| message(ErrorCode::UnknownType, value.span, None, &value.text))
        .collect()
}

#[cfg(test)]
mod tests;

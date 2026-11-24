//! The bodies of `GET /api/search` and `GET /api/search/fields`, and the
//! search errors a problem document lists.

use crate::wire::ShareSummary;
use flare_query::{Part, QueryError, Span, Term};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The characters a part or an error covers in the query, counted in
/// Unicode scalar values from 0. `end` is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchSpan {
    /// The first character.
    pub start: u32,
    /// The character after the last one.
    pub end: u32,
}

/// One value of a filter, with its span. A quoted value's span includes
/// its quotes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchValue {
    /// The value, without quotes.
    pub text: String,
    /// Where it was typed.
    pub span: SearchSpan,
}

/// One parsed part of a search, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[non_exhaustive]
pub enum SearchPart {
    /// A word, which matches inside a title in any case.
    Word {
        /// The word.
        text: String,
        /// Whether a leading minus excludes what it matches.
        negated: bool,
        /// Where it was typed, minus included.
        span: SearchSpan,
    },
    /// A quoted phrase, which matches as one string.
    Phrase {
        /// The phrase, without quotes.
        text: String,
        /// Whether a leading minus excludes what it matches.
        negated: bool,
        /// Where it was typed, minus and quotes included.
        span: SearchSpan,
    },
    /// A filter, which matches when any of its values does.
    Filter {
        /// The field, such as `from`.
        field: String,
        /// The values, at least one.
        values: Vec<SearchValue>,
        /// Whether a leading minus excludes what it matches.
        negated: bool,
        /// Where it was typed, minus included.
        span: SearchSpan,
    },
}

/// `GET /api/search?q=<text>&limit=N`: the shares the caller can open that
/// match the query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchAnswer {
    /// The query's canonical text.
    pub query: String,
    /// The query's parts, in the order they were typed.
    pub parsed: Vec<SearchPart>,
    /// The matching shares, newest first.
    pub results: Vec<ShareSummary>,
    /// The cursor of the next page. A search answers one page, so it is
    /// always null.
    pub next: Option<String>,
    /// Whether the caller's own shares or the shares granted to the caller
    /// passed the 500 that one search reads, so a match may be missing.
    pub partial: bool,
}

/// What a field's values are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    /// One of the listed values.
    Keyword,
    /// A person: `me`, an email address or a name the server knows.
    Person,
}

/// One field a search can filter on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchField {
    /// The field's name, such as `type`.
    pub name: String,
    /// What its values are.
    pub kind: FieldKind,
    /// The values a builder offers.
    pub values: Vec<String>,
    /// What it matches, in one sentence.
    pub about: String,
}

/// `GET /api/search/fields`: every field a search can filter on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchFields {
    /// The fields, in the order a builder offers them.
    pub fields: Vec<SearchField>,
}

/// One error in a search's text, which a problem document lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchError {
    /// The stable code, such as `flare::search::unknown_filter`.
    pub code: String,
    /// The characters it covers.
    pub span: SearchSpan,
    /// How to fix it, in one sentence.
    pub message: String,
}

impl From<Span> for SearchSpan {
    fn from(span: Span) -> Self {
        let point = |at: usize| u32::try_from(at).unwrap_or(u32::MAX);
        Self {
            start: point(span.start),
            end: point(span.end),
        }
    }
}

impl From<&Part> for SearchPart {
    fn from(part: &Part) -> Self {
        let (negated, span) = (part.negated, SearchSpan::from(part.span));
        match &part.term {
            Term::Word { text } => Self::Word {
                text: text.clone(),
                negated,
                span,
            },
            Term::Phrase { text } => Self::Phrase {
                text: text.clone(),
                negated,
                span,
            },
            Term::Filter { field, values } => Self::Filter {
                field: field.name().to_owned(),
                values: values
                    .iter()
                    .map(|value| SearchValue {
                        text: value.text.clone(),
                        span: value.span.into(),
                    })
                    .collect(),
                negated,
                span,
            },
        }
    }
}

impl From<&QueryError> for SearchError {
    fn from(error: &QueryError) -> Self {
        Self {
            code: error.code.text().to_owned(),
            span: error.span.into(),
            message: error.message.clone(),
        }
    }
}

#[cfg(test)]
mod tests;

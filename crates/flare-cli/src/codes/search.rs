//! The failures of `flare search`: a search with an error is a [`Refused`]
//! with `flare::search::invalid` or `flare::search::unknown_person`, whose
//! causes print the query with a caret line under each error.
//!
//! Shortcut: each caret line counts one column for each Unicode scalar
//! value, so a caret under text after a wide character, such as one of
//! Chinese or Japanese, sits to its left. crumbkit's miette labels would
//! measure columns, and they need the miette crate, which hangar has not
//! evaluated.

use super::{Failed, Refused};
use crumbkit::{Code, Field, FieldValue};
use flare_contract::{ApiCode, SearchError};
use flare_query::QueryError;

/// A search that does not parse, or names a type that does not exist,
/// which ends the run with exit code 2.
pub const SEARCH_INVALID: Code<'static> = Code::from_spelling("flare::search::invalid");

/// The query of a refused search and its errors, which the refusal's
/// causes print.
#[derive(Debug, Default)]
pub(crate) struct Searched {
    /// The query as the caller typed it, once flare knows it.
    query: Option<String>,
    /// The errors, in the order of the query.
    errors: Vec<SearchError>,
}

impl Searched {
    /// The query, then for each error a line with spaces up to its span,
    /// a caret under each of its characters, and its message. Without the
    /// query, each error prints its span and its message.
    pub(crate) fn causes(&self) -> Vec<String> {
        if self.errors.is_empty() {
            return Vec::new();
        }
        let Some(query) = &self.query else {
            return self
                .errors
                .iter()
                .map(|error| {
                    let span = error.span;
                    format!(
                        "characters {} to {}: {}",
                        span.start, span.end, error.message
                    )
                })
                .collect();
        };
        let lines = self.errors.iter().map(|error| {
            let start = usize::try_from(error.span.start).unwrap_or(0);
            let end = usize::try_from(error.span.end).unwrap_or(start);
            let width = end.saturating_sub(start).max(1);
            format!(
                "{}{} {}",
                " ".repeat(start),
                "^".repeat(width),
                error.message
            )
        });
        std::iter::once(query.clone()).chain(lines).collect()
    }

    /// The query, once flare knows it, and the errors, each with its code,
    /// span and message, so an agent reads them without parsing the causes.
    pub(crate) fn fields(&self) -> Vec<Field<'_>> {
        if self.errors.is_empty() {
            return Vec::new();
        }
        let errors = self.errors.iter().map(|error| {
            FieldValue::object([
                ("code", FieldValue::from(error.code.as_str())),
                ("start", FieldValue::from(error.span.start)),
                ("end", FieldValue::from(error.span.end)),
                ("message", FieldValue::from(error.message.as_str())),
            ])
        });
        let query = self.query.as_ref().map(|query| Field::new("query", query));
        query
            .into_iter()
            .chain([Field::new("errors", FieldValue::list(errors))])
            .collect()
    }
}

impl Refused {
    /// The same refusal listing the errors of a search that did not run.
    pub(crate) fn with_errors(mut self, errors: Vec<SearchError>) -> Self {
        self.searched.errors = errors;
        self
    }

    /// The refusal of the search `query` for its `errors`, found before
    /// any request.
    pub(crate) fn invalid_search(query: &str, errors: &[QueryError]) -> Self {
        let mut refused = Self::new(ApiCode::SearchInvalid, None, "")
            .with_errors(errors.iter().map(SearchError::from).collect());
        refused.searched.query = Some(query.to_owned());
        refused
    }
}

impl Failed {
    /// The same failure of the search `query`, when it is a refusal of the
    /// share API, so its causes print `query` with the caret lines.
    pub(crate) fn for_search(self, query: &str) -> Self {
        match self {
            Self::Refused(mut refused) => {
                refused.searched.query = Some(query.to_owned());
                Self::Refused(refused)
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests;

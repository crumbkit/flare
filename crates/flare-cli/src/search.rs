//! `flare search`: parses the query and checks its types, then asks the
//! saved login's server for the shares the caller can open that match it,
//! and prints them as `flare shares` prints its rows.
//!
//! A query with an error fails with `flare::search::invalid` before any
//! request, and needs no login, so a person fixes the text first.

use crate::cli::SearchArgs;
use crate::codes::{Failed, Refused};
use crate::documents::ShareRow;
use crate::env::Snapshot;
use crate::sharing::{body, row, session};
use crumbkit::{Document, Listing};
use flare_contract::SearchAnswer;
use flare_query::fields::check;
use flare_query::parse;
use reqwest::Method;
use schemars::JsonSchema;
use serde::Serialize;
use std::fmt;

/// The most shares one search answers, which `--all` asks for.
const MAX_RESULTS: usize = 500;

/// What a search printed besides its rows.
#[derive(Debug, Serialize, JsonSchema)]
pub(crate) struct SearchSummary {
    /// The query's canonical text, as the server read it.
    pub(crate) query: String,
    /// Whether your own shares or the shares granted to you passed the 500
    /// that one search reads, so a match may be missing.
    pub(crate) partial: bool,
}

/// The text output prints a line only for a partial search.
impl fmt::Display for SearchSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.partial {
            formatter.write_str(
                "You own or were granted more than 500 shares, and the search read 500 of each, so a match may be missing.",
            )?;
        }
        Ok(())
    }
}

/// The document of `flare search`.
pub(crate) const SEARCH: Document<Listing<ShareRow, SearchSummary>> = Document::new(
    "flare",
    "search",
    1,
    "The shares the caller can open that match a search, newest first, and the search's canonical text.",
)
.when_empty("No shares match.");

/// Runs `flare search`.
///
/// # Errors
///
/// Returns `flare::search::invalid` before any request for a query that
/// does not parse or names a type flare does not serve, and the failure
/// of the request otherwise.
pub(crate) async fn search(
    args: &SearchArgs,
    env: &Snapshot,
) -> Result<Listing<ShareRow, SearchSummary>, Failed> {
    let text = args.query.as_str();
    let query = parse(text).map_err(|errors| Refused::invalid_search(text, &errors))?;
    let unknown = check(&query);
    if !unknown.is_empty() {
        return Err(Refused::invalid_search(text, &unknown).into());
    }
    let api = session(env)?;
    let limit = args.page.limit().unwrap_or(MAX_RESULTS).to_string();
    let pairs = form_urlencoded::Serializer::new(String::new())
        .append_pair("q", text)
        .append_pair("limit", &limit)
        .finish();
    let target = format!("/api/search?{pairs}");
    let answer: Option<SearchAnswer> = api
        .send(Method::GET, &target, None::<&()>)
        .await
        .map_err(|failed| failed.for_search(text))?;
    let answer = body(&api, answer)?;
    let rows = answer.results.into_iter().map(row).collect();
    Ok(Listing::uncounted(rows, false)
        .described("shares")
        .summary(SearchSummary {
            query: answer.query,
            partial: answer.partial,
        }))
}

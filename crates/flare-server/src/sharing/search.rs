//! `GET /api/search` and `GET /api/search/fields`: a search over the
//! shares the caller can open, and the fields a search filters on.
//!
//! The query is parsed before anything is read, and a query with an error
//! answers 422 with every error and runs nothing. `Flares::search` then
//! checks each `type:` value and resolves each `from:` value before it
//! reads the caller's two lists of shares.

use super::convert::{share_error, standing, summary};
use super::run::json;
use super::{Answer, Call, Refusal, Sharing};
use flare_contract::{
    ApiCode, FieldKind as WireKind, SearchAnswer, SearchError as WireError, SearchField,
    SearchFields, SearchPart,
};
use flare_core::{
    GrantStore, Principal, Recipients, ResponseStore, SearchError, ShareStore, TokenStore,
};
use flare_query::fields::{FIELDS, FieldKind};
use flare_query::{ErrorCode, QueryError, parse};

/// The call of `GET /api/search` from the query's `q` values and its
/// limit. The query needs exactly one `q`, which may be empty.
pub(crate) fn search_call(texts: Vec<String>, limit: usize) -> Result<Call, Refusal> {
    let [text]: [String; 1] = texts.try_into().map_err(|_| ApiCode::BadQuery)?;
    Ok(Call::Search { text, limit })
}

/// The 422 refusal that lists `errors`: `unknown_person` when every error
/// names an unknown person, and `invalid` otherwise.
fn invalid(errors: &[QueryError]) -> Refusal {
    let people = errors
        .iter()
        .all(|error| error.code == ErrorCode::UnknownPerson);
    let code = if people {
        ApiCode::UnknownPerson
    } else {
        ApiCode::SearchInvalid
    };
    Refusal {
        code,
        detail: None,
        share: None,
        errors: errors.iter().map(WireError::from).collect(),
    }
}

/// The refusal for a search that did not run.
fn search_error(error: SearchError) -> Refusal {
    match error {
        SearchError::Invalid(errors) => invalid(&errors),
        SearchError::RecipientsUnavailable => ApiCode::RecipientsUnavailable.into(),
        SearchError::Share(error) => share_error(error).into(),
        _ => ApiCode::SharesUnavailable.into(),
    }
}

/// The answer of `GET /api/search/fields`.
pub(super) fn fields() -> Answer {
    let fields = FIELDS
        .iter()
        .map(|info| SearchField {
            name: info.field.name().to_owned(),
            kind: match info.kind {
                FieldKind::Keyword => WireKind::Keyword,
                FieldKind::Person => WireKind::Person,
            },
            values: info
                .values
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            about: info.about.to_owned(),
        })
        .collect();
    json(200, &SearchFields { fields })
}

impl<S, G, R, T, X> Sharing<S, G, R, T, X>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    T: TokenStore,
    X: Recipients,
{
    /// The answer of `GET /api/search` for `text`, with at most `limit`
    /// results.
    pub(super) async fn search(&self, who: &Principal, text: &str, limit: usize) -> Answer {
        let query = parse(text).map_err(|errors| invalid(&errors))?;
        let found = self
            .flares
            .search(who, &query, &self.recipients, limit)
            .await
            .map_err(search_error)?;
        let results = found
            .hits
            .iter()
            .map(|hit| summary(&hit.share, standing(hit.standing)))
            .collect();
        json(
            200,
            &SearchAnswer {
                query: query.to_string(),
                parsed: query.parts().iter().map(SearchPart::from).collect(),
                results,
                next: None,
                partial: found.partial,
            },
        )
    }
}

#[cfg(test)]
mod tests;

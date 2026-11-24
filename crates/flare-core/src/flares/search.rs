//! [`Flares::search`]: a parsed search run over the shares the caller can
//! already open.
//!
//! The candidates are the caller's own shares and the shares granted to the
//! caller, read through [`Shares::mine`](crate::Shares::mine) and
//! [`Shares::shared_with_me`](crate::Shares::shared_with_me), the same
//! calls `flare shares` uses. A share the caller cannot open is never a
//! candidate, so no query text can widen what the caller sees, and a minus
//! part only narrows the candidates.

use super::{Flares, RecipientError, Recipients};
use crate::auth::Principal;
use crate::grant::Role;
use crate::name::PrincipalId;
use crate::share::{Share, ShareKind};
use crate::sharing::{ShareError, Standing};
use crate::store::{GrantStore, ResponseStore, ShareStore};
use crate::teams::Teams;
use flare_query::fields::{ShareType, check, unknown_person};
use flare_query::{Field, Query, QueryError, Term, Value};
use std::cmp::Reverse;

/// The most hits one search answers.
pub const MAX_HITS: usize = 500;

/// One share a search found, with what the caller is to it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SearchHit {
    /// The share.
    pub share: Share,
    /// What the caller is to it: its owner, an admin or a reader.
    pub standing: Standing,
}

/// What a search found.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Found {
    /// The hits, newest share first.
    pub hits: Vec<SearchHit>,
    /// Whether the caller's own list or the list of shares granted to the
    /// caller stopped at [`MAX_LISTED`](crate::MAX_LISTED), so the search
    /// did not see every share the caller can open.
    pub partial: bool,
}

/// Why a search did not run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SearchError {
    /// A `type:` value names no kind of share, or a `from:` value names
    /// nobody the deployment knows. Each error names its span.
    #[error("the search names a type or a person that does not exist")]
    Invalid(Vec<QueryError>),
    /// The recipients directory did not answer.
    #[error("the recipients directory is unavailable")]
    RecipientsUnavailable,
    /// The share service refused or a store did not answer.
    #[error(transparent)]
    Share(ShareError),
}

/// What one part of a query tests, ready to run against a share.
enum Test {
    /// The title holds this text, in lower case.
    Text(String),
    /// The share is of one of these kinds.
    Types(Vec<ShareType>),
    /// The share's owner is one of these people.
    People(Vec<PrincipalId>),
}

/// One part of a query, ready to run.
struct Clause {
    test: Test,
    negated: bool,
}

impl Clause {
    fn admits(&self, share: &Share) -> bool {
        let matched = match &self.test {
            Test::Text(text) => share.title().as_str().to_lowercase().contains(text),
            Test::Types(types) => types.iter().any(|kind| match (kind, share.kind()) {
                (ShareType::Markdown, ShareKind::Document) => true,
                // flare serves no HTML shares yet, so html matches none.
                _ => false,
            }),
            Test::People(people) => people.contains(share.owner()),
        };
        matched != self.negated
    }
}

/// The person each `from:` value of `query` names: `me` is the caller, and
/// any other value is what `recipients` resolves.
async fn resolve<X: Recipients>(
    who: &Principal,
    values: &[Value],
    recipients: &X,
    errors: &mut Vec<QueryError>,
) -> Result<Vec<PrincipalId>, SearchError> {
    let mut people = Vec::with_capacity(values.len());
    for value in values {
        if value.text.eq_ignore_ascii_case("me") {
            let me = who
                .email()
                .ok_or(SearchError::Share(ShareError::NoVerifiedEmail))?;
            people.push(me.principal_id());
            continue;
        }
        match recipients.resolve(&value.text).await {
            Ok(email) => people.push(email.principal_id()),
            Err(RecipientError::NotAnAddress) => errors.push(unknown_person(value)),
            Err(_) => return Err(SearchError::RecipientsUnavailable),
        }
    }
    Ok(people)
}

/// The clauses of `query`, after every `type:` value is checked and every
/// `from:` value resolved.
async fn compile<X: Recipients>(
    who: &Principal,
    query: &Query,
    recipients: &X,
) -> Result<Vec<Clause>, SearchError> {
    let mut errors = check(query);
    let mut clauses = Vec::with_capacity(query.parts().len());
    for part in query.parts() {
        let test = match &part.term {
            Term::Word { text } | Term::Phrase { text } => Test::Text(text.to_lowercase()),
            Term::Filter {
                field: Field::Type,
                values,
            } => Test::Types(
                values
                    .iter()
                    .filter_map(|value| ShareType::named(&value.text))
                    .collect(),
            ),
            Term::Filter {
                field: Field::From,
                values,
            } => Test::People(resolve(who, values, recipients, &mut errors).await?),
        };
        clauses.push(Clause {
            test,
            negated: part.negated,
        });
    }
    if errors.is_empty() {
        Ok(clauses)
    } else {
        errors.sort_by_key(|error| error.span.start);
        Err(SearchError::Invalid(errors))
    }
}

/// What a caller is to a share granted with `role`.
fn granted(role: Role) -> Standing {
    match role {
        Role::Admin => Standing::Admin,
        Role::Reader => Standing::Reader,
    }
}

impl<S: ShareStore, G: GrantStore, R: ResponseStore, C, U, T: Teams, K>
    Flares<S, G, R, C, U, T, K>
{
    /// The shares `who` can open that match every part of `query`, newest
    /// first and then by id, at most `limit` and at most [`MAX_HITS`]. A
    /// `limit` of 0 answers one. `recipients` resolves each `from:` value
    /// other than `me`.
    ///
    /// # Errors
    ///
    /// Returns [`SearchError::Invalid`] with every unknown type and person
    /// before any share is read, [`SearchError::RecipientsUnavailable`],
    /// and [`SearchError::Share`] with [`ShareError::NoVerifiedEmail`] or
    /// [`ShareError::Unavailable`].
    pub async fn search<X: Recipients>(
        &self,
        who: &Principal,
        query: &Query,
        recipients: &X,
        limit: usize,
    ) -> Result<Found, SearchError> {
        let clauses = compile(who, query, recipients).await?;
        let mine = self.shares.mine(who).await.map_err(SearchError::Share)?;
        let granted_to_me = self
            .shares
            .shared_with_me(who, false)
            .await
            .map_err(SearchError::Share)?;
        let partial = mine.truncated() || granted_to_me.truncated();
        let owned = mine.into_items().into_iter().map(|share| SearchHit {
            share,
            standing: Standing::Owner,
        });
        let others = granted_to_me
            .into_items()
            .into_iter()
            .map(|entry| SearchHit {
                share: entry.share,
                standing: granted(entry.role),
            });
        let mut hits: Vec<SearchHit> = owned
            .chain(others)
            .filter(|hit| clauses.iter().all(|clause| clause.admits(&hit.share)))
            .collect();
        hits.sort_by(|one, two| {
            (Reverse(one.share.created()), one.share.id())
                .cmp(&(Reverse(two.share.created()), two.share.id()))
        });
        hits.dedup_by(|one, two| one.share.id() == two.share.id());
        hits.truncate(limit.clamp(1, MAX_HITS));
        Ok(Found { hits, partial })
    }
}

#[cfg(test)]
mod tests;

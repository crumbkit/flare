//! [`Sharing`]: the share API's service, token store and recipients
//! directory, which [`App::with_sharing`](crate::App::with_sharing) adds to
//! a hosted site.
//!
//! The handlers reach it through [`Api`], a trait with boxed futures, so
//! the app keeps its four type parameters and no handler names a store
//! type. `run.rs` holds the generic implementation, `convert.rs` turns
//! flare-core's values and refusals into the contract's, and `tokens.rs`
//! mints and reads tokens and login codes.
//!
//! A token's text is `flare_<id>_<secret>`: the id is 32 hexadecimal
//! characters and the secret 43 base64url characters of 32 random bytes.
//! The store keeps the SHA-256 of the secret. A login code is 43 base64url
//! characters of 32 random bytes, and the store keeps its SHA-256.

mod convert;
mod run;
mod tokens;

pub(crate) use tokens::{LoginRedirect, bearer_of};

use flare_auth_oidc::Admit;
use flare_contract::{ApiCode, ExchangeRequest, GrantRequest, ShareRequest};
use flare_core::{Email, Flares, Principal, Timestamp};
use secrecy::SecretString;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// A boxed future that the [`Api`] trait returns.
pub(crate) type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The share API's parts: the [`Flares`] service, the token store, the
/// directory that resolves `--with` identifiers and the admit rule that
/// every token's owner must still pass.
pub struct Sharing<S, G, R, T, X> {
    flares: Flares<S, G, R>,
    tokens: T,
    recipients: X,
    admit: Admit,
}

impl<S, G, R, T, X> Sharing<S, G, R, T, X> {
    /// The share API over `flares`, keeping tokens in `tokens`, resolving
    /// recipients with `recipients` and accepting a token only while
    /// `admit`, the site's sign-in rule, admits its owner's address.
    #[must_use]
    pub fn new(flares: Flares<S, G, R>, tokens: T, recipients: X, admit: Admit) -> Self {
        Self {
            flares,
            tokens,
            recipients,
            admit,
        }
    }
}

impl<S, G, R, T, X> fmt::Debug for Sharing<S, G, R, T, X> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Sharing").finish_non_exhaustive()
    }
}

/// Which shares a list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    /// The caller's own shares.
    Mine,
    /// The shares granted to the caller.
    WithMe,
}

/// One request to the share API, parsed from its route, query and body.
#[derive(Debug)]
pub(crate) enum Call {
    /// `POST /api/shares`.
    Create(ShareRequest),
    /// `GET /api/shares`.
    List { scope: Scope, limit: usize },
    /// `GET /api/shares/{id}`.
    Open(String),
    /// `POST /api/grants/{id}`.
    Grant(String, GrantRequest),
    /// `GET /api/activity`.
    Activity {
        flares: Vec<String>,
        since: Option<String>,
        limit: usize,
    },
    /// `DELETE /api/tokens/{id}`.
    Revoke(String),
}

/// A refusal of the share API: its code, and what the document adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) code: ApiCode,
    pub(crate) detail: Option<String>,
    pub(crate) share: Option<String>,
}

impl From<ApiCode> for Refusal {
    fn from(code: ApiCode) -> Self {
        Self {
            code,
            detail: None,
            share: None,
        }
    }
}

/// An answer of the share API: a status and a JSON body, or none.
pub(crate) type Answer = Result<(u16, Option<String>), Refusal>;

/// What a bearer token proved.
#[derive(Debug)]
pub(crate) enum TokenCheck {
    /// An active flare token of this principal.
    Principal(Principal),
    /// The text is not a flare token, so the deployment's authenticator
    /// decides.
    NotOurs,
    /// A flare token that does not authenticate, whose owner the admit
    /// rule no longer admits, or a store failure.
    Refused(ApiCode),
}

/// One version of a Markdown share, as the reading page shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reading {
    /// The share's title.
    pub(crate) title: String,
    /// The owner's address.
    pub(crate) owner: String,
    /// Whether the reader is the owner.
    pub(crate) own: bool,
    /// The addresses the share is granted to, for its owner and its
    /// admins. A reader may not list the grants, so the page says it is
    /// shared with them.
    pub(crate) with: Option<Vec<String>>,
    /// The version shown.
    pub(crate) version: u32,
    /// When the version was published, in seconds since 1970.
    pub(crate) published: u64,
    /// The version's Markdown.
    pub(crate) markdown: String,
}

/// Why the reading page cannot show a share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadRefusal {
    /// The share is missing, the reader has no standing in it, the version
    /// does not exist or the share is not Markdown. Each answers the same,
    /// so a reader cannot tell a refused share from a missing one.
    NotFound,
    /// A store did not answer.
    Unavailable,
}

/// The share API with its stores' types erased.
pub(crate) trait Api: Send + Sync {
    /// Checks a bearer token at `now`.
    fn authenticate<'a>(&'a self, token: &'a SecretString, now: Timestamp)
    -> Boxed<'a, TokenCheck>;

    /// Issues a login code for `owner` and the CLI on `port`.
    fn issue_code<'a>(
        &'a self,
        owner: &'a Email,
        port: u16,
        now: Timestamp,
    ) -> Boxed<'a, Result<SecretString, ApiCode>>;

    /// Exchanges a login code for a token.
    fn exchange(&self, request: ExchangeRequest, now: Timestamp) -> Boxed<'_, Answer>;

    /// Runs one request of an authenticated caller.
    fn run<'a>(&'a self, who: &'a Principal, call: Call, now: Timestamp) -> Boxed<'a, Answer>;

    /// Opens share `id` for the reading page: version `version` when the
    /// query names one, and the latest otherwise.
    fn read<'a>(
        &'a self,
        who: &'a Principal,
        id: &'a str,
        version: Option<&'a str>,
    ) -> Boxed<'a, Result<Reading, ReadRefusal>>;
}

#[cfg(test)]
mod tests;

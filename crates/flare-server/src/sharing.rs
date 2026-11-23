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
//! [`check_share`] and [`check_version`] are the checks that a create and a
//! new version apply to their body before any store call. The `flare share
//! --dry-run` command calls the same two functions, so a dry run refuses
//! what the server would refuse.
//!
//! A token's text is `flare_<id>_<secret>`: the id is 32 hexadecimal
//! characters and the secret 43 base64url characters of 32 random bytes.
//! The store keeps the SHA-256 of the secret. A login code is 43 base64url
//! characters of 32 random bytes, and the store keeps its SHA-256.

mod convert;
mod grants;
mod run;
mod teams;
mod tokens;

pub(crate) use tokens::{LoginRedirect, bearer_of};

use convert::content_error;

use flare_auth_oidc::Admit;
use flare_contract::{
    ApiCode, ExchangeRequest, Format, GrantRequest, MembersRequest, RevokeRequest, ShareRequest,
    TeamRequest, VersionRequest,
};
use flare_core::{
    Admission, DocumentText, Email, Flares, MailDomain, NoTeams, Principal, ShareTitle,
    ShareVersion, Timestamp,
};
use secrecy::SecretString;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// A boxed future that the [`Api`] trait returns.
pub(crate) type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The share API's parts: the [`Flares`] service with its teams, the token
/// store, the directory that resolves `--with` identifiers and the admit
/// rule that every token's owner must still pass.
pub struct Sharing<S, G, R, T, X, M = NoTeams> {
    flares: Flares<S, G, R, M>,
    tokens: T,
    recipients: X,
    admit: Admit,
}

/// The share service's view of the site's admit rule, which an everyone
/// grant records as its audience.
fn admission(admit: &Admit) -> Admission {
    match admit {
        Admit::Emails(_) => Admission::Listed,
        Admit::Domain(_) => admit
            .domain()
            .and_then(|domain| MailDomain::try_from(domain.as_str()).ok())
            .map_or(Admission::Anyone, Admission::Domain),
        _ => Admission::Anyone,
    }
}

impl<S, G, R, T, X, M> Sharing<S, G, R, T, X, M> {
    /// The share API over `flares`, keeping tokens in `tokens`, resolving
    /// recipients with `recipients` and accepting a token only while
    /// `admit`, the site's sign-in rule, admits its owner's address. The
    /// share service writes and reads everyone grants under `admit`.
    #[must_use]
    pub fn new(flares: Flares<S, G, R, M>, tokens: T, recipients: X, admit: Admit) -> Self {
        Self {
            flares: flares.admitting(admission(&admit)),
            tokens,
            recipients,
            admit,
        }
    }
}

impl<S, G, R, T, X, M> fmt::Debug for Sharing<S, G, R, T, X, M> {
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
    List {
        scope: Scope,
        limit: usize,
        everyone: bool,
    },
    /// `GET /api/shares/{id}`, at a version or at the latest.
    Open(String, Option<ShareVersion>),
    /// `POST /api/shares/{id}/versions`.
    Publish(String, VersionRequest),
    /// `POST /api/grants/{id}`.
    Grant(String, GrantRequest),
    /// `GET /api/grants/{id}`.
    ListGrants(String),
    /// `DELETE /api/grants/{id}`.
    RevokeGrants(String, RevokeRequest),
    /// `POST /api/teams`.
    CreateTeam(TeamRequest),
    /// `GET /api/teams`.
    ListTeams,
    /// `GET /api/teams/{name}`.
    ShowTeam(String),
    /// `DELETE /api/teams/{name}`.
    DeleteTeam(String),
    /// `POST /api/teams/{name}/members`.
    AddMembers(String, MembersRequest),
    /// `DELETE /api/teams/{name}/members`.
    RemoveMembers(String, MembersRequest),
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

/// Why a share's body fails the checks of [`check_share`] or
/// [`check_version`]: the code the API answers with, and its detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyRefused {
    /// The code, such as `flare::shares::too_large`.
    pub code: ApiCode,
    /// What is wrong, when the code alone does not say.
    pub detail: Option<String>,
}

impl From<BodyRefused> for Refusal {
    fn from(refused: BodyRefused) -> Self {
        Self {
            code: refused.code,
            detail: refused.detail,
            share: None,
        }
    }
}

/// The checks a create applies to its body before any store call: the
/// format, then the title, then the content.
///
/// # Errors
///
/// Returns `flare::shares::kind_not_served` for HTML,
/// `flare::shares::too_large` for content over 256 KiB, and
/// `flare::shares::invalid` with a detail for any other title or content
/// that its type refuses.
pub fn check_share(request: &ShareRequest) -> Result<(ShareTitle, DocumentText), BodyRefused> {
    served(request.format)?;
    let title = ShareTitle::try_from(request.title.as_str()).map_err(|e| content_error(&e))?;
    let text = DocumentText::try_from(request.content.as_str()).map_err(|e| content_error(&e))?;
    Ok((title, text))
}

/// The checks a new version applies to its body before any store call:
/// the format, then the content.
///
/// # Errors
///
/// Returns the refusals of [`check_share`] other than the title's.
pub fn check_version(request: &VersionRequest) -> Result<DocumentText, BodyRefused> {
    served(request.format)?;
    DocumentText::try_from(request.content.as_str()).map_err(|e| content_error(&e))
}

/// Refuses a format the server does not serve.
fn served(format: Format) -> Result<(), BodyRefused> {
    match format {
        Format::Markdown => Ok(()),
        Format::Html => Err(BodyRefused {
            code: ApiCode::KindNotServed,
            detail: None,
        }),
    }
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
}

#[cfg(test)]
mod tests;

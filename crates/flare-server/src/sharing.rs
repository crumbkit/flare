//! [`Sharing`]: the share API's service, token store and recipients
//! directory, which [`App::with_sharing`](crate::App::with_sharing) adds to
//! a hosted site.
//!
//! The handlers reach it through [`Api`], a trait with boxed futures, so
//! the app keeps its four type parameters and no handler names a store
//! type. `run.rs` holds the generic implementation, `read.rs` the reading
//! page's read and its get flare address, `convert.rs` turns
//! flare-core's values and refusals into the contract's, and `tokens.rs`
//! mints and reads tokens and login codes. `upload.rs` runs a share or a
//! version sent with its images, and `files.rs` holds the file store a
//! site without one uses and the file route's read.
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

mod activity;
mod comments;
mod convert;
mod files;
mod grants;
mod list;
mod read;
mod run;
mod search;
mod teams;
mod tokens;
mod upload;

pub use files::NoFiles;
pub(crate) use list::list_after;
pub use read::{GetFlare, NotGetFlare};
pub(crate) use search::search_call;
pub(crate) use tokens::{LoginRedirect, bearer_of};
pub(crate) use upload::{ImagePart, Upload, Uploaded};

use convert::content_error;

use crate::clock::Clock;
use flare_auth_oidc::Admit;
use flare_contract::{
    ApiCode, CommentEdit, ExchangeRequest, Format, GrantRequest, MembersRequest, NamedCursor,
    NewComment, RevokeRequest, ShareRequest, TeamRequest, TransferRequest, VersionRequest,
};
use flare_core::{
    Admission, AllAdmitted, DocumentText, Email, Flares, ListKey, MailDomain, NoTeams, Principal,
    ShareTitle, ShareVersion, StoredFile, Timestamp,
};
use secrecy::SecretString;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// A boxed future that the [`Api`] trait returns.
pub(crate) type Boxed<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The share API's parts: the [`Flares`] service with its teams and its
/// creators rule, the token store, the directory that resolves `--with`
/// identifiers, the admit rule that every token's owner must still pass,
/// and the get flare address that [`Sharing::with_get_flare`] sets.
pub struct Sharing<S, G, R, C, U, T, X, M = NoTeams, K = AllAdmitted, F = NoFiles> {
    flares: Flares<S, G, R, C, U, M, K>,
    files: F,
    tokens: T,
    recipients: X,
    admit: Admit,
    get_flare: Option<GetFlare>,
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

impl<S, G, R, C, U, T, X, M, K> Sharing<S, G, R, C, U, T, X, M, K> {
    /// The share API over `flares`, keeping tokens in `tokens`, resolving
    /// recipients with `recipients` and accepting a token only while
    /// `admit`, the site's sign-in rule, admits its owner's address. The
    /// share service writes and reads everyone grants under `admit`. Its
    /// reading pages link to no get flare address.
    #[must_use]
    pub fn new(
        flares: Flares<S, G, R, C, U, M, K>,
        tokens: T,
        recipients: X,
        admit: Admit,
    ) -> Self {
        Self {
            flares: flares.admitting(admission(&admit)),
            files: NoFiles,
            tokens,
            recipients,
            admit,
            get_flare: None,
        }
    }
}

impl<S, G, R, C, U, T, X, M, K, F> fmt::Debug for Sharing<S, G, R, C, U, T, X, M, K, F> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Sharing").finish_non_exhaustive()
    }
}

/// Which shares a list shows: the caller's own, or those granted to the
/// caller. It is the contract's scope, which a share cursor also names.
pub(crate) use flare_contract::ListScope as Scope;

/// One request to the share API, parsed from its route, query and body.
#[derive(Debug)]
pub(crate) enum Call {
    /// `POST /api/shares`.
    Create(ShareRequest),
    /// `GET /api/shares`, from the first entry or after a key.
    List {
        scope: Scope,
        after: Option<ListKey>,
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
    /// `PUT /api/teams/{name}/owner`.
    TransferTeam(String, TransferRequest),
    /// `POST /api/teams/{name}/members`.
    AddMembers(String, MembersRequest),
    /// `DELETE /api/teams/{name}/members`.
    RemoveMembers(String, MembersRequest),
    /// `GET /api/activity`.
    Activity {
        flares: Vec<String>,
        since: Option<String>,
        since_last: Option<String>,
        limit: usize,
    },
    /// `GET /api/comments`.
    Comments(String),
    /// `POST /api/comments`.
    Post(NewComment),
    /// `PATCH /api/comments/{id}`.
    Edit(String, CommentEdit),
    /// `DELETE /api/comments/{id}`.
    Delete(String),
    /// `GET /api/cursors/{name}`.
    Cursor(String),
    /// `PUT /api/cursors/{name}`.
    SaveCursor(String, NamedCursor),
    /// `DELETE /api/tokens/{id}`.
    Revoke(String),
    /// `GET /api/search`, with its query text, the most results and
    /// whether to search the shares open to everyone signed in.
    Search {
        text: String,
        limit: usize,
        everyone: bool,
    },
    /// `GET /api/search/fields`.
    SearchFields,
}

/// A refusal of the share API: its code, and what the document adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) code: ApiCode,
    pub(crate) detail: Option<String>,
    pub(crate) share: Option<String>,
    /// Each error of a search that did not run.
    pub(crate) errors: Vec<flare_contract::SearchError>,
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
            errors: Vec::new(),
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
            errors: Vec::new(),
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

/// The comments and replies of a share, as the reading page counts them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Count {
    /// The top-level comments the section shows.
    pub(crate) comments: u32,
    /// The replies that are not deleted.
    pub(crate) replies: u32,
}

/// One version of a Markdown share, as the reading page shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reading {
    /// The share's id, which the comments section reads its thread by.
    pub(crate) id: String,
    /// The share's comments and replies as its comments section counts
    /// them, or `None` when the comment store did not answer.
    pub(crate) comments: Option<Count>,
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
    /// Where the page loads each image the version lists.
    pub(crate) images: flare_render::ImageSources,
    /// The get flare address the page's foot links to, for a reader who
    /// has never shared a flare on a site that sets one.
    pub(crate) badge: Option<String>,
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

    /// Whether the site's admit rule still admits `who`, which a session
    /// cookie must pass as a bearer token does.
    fn admits(&self, who: &Principal) -> bool;

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

    /// Runs one upload, a share or a version sent with its images, of an
    /// authenticated caller. The upload refuses to commit once 5 minutes
    /// have passed on `clock` since it started.
    fn upload<'a>(
        &'a self,
        who: &'a Principal,
        upload: Upload,
        now: Timestamp,
        clock: &'a dyn Clock,
    ) -> Boxed<'a, Uploaded>;

    /// File `file` of version `version` of share `id`, when the caller may
    /// open that version and its image table lists the file.
    fn file<'a>(
        &'a self,
        who: &'a Principal,
        id: &'a str,
        version: &'a str,
        file: &'a str,
    ) -> Boxed<'a, Result<StoredFile, ReadRefusal>>;
}

#[cfg(test)]
mod tests;

//! The share API's routes on the hosted site, and `/cli/login`, which
//! hands a signed-in browser's login code to the `flare` command.
//!
//! Every `/api/` route but the code exchange takes a bearer token. The
//! comment routes and `/api/activity` also take the session cookie when
//! the request has no Authorization header, after `hosted::check` has
//! required the headers that a page of this site sends. No other `/api/`
//! path reads a cookie, so a page on another site cannot make a browser act
//! through them. A token that is not a flare token goes to the
//! deployment's authenticator, which may accept bearer tokens of its own.
//! Every refusal is an RFC 9457 problem document with a
//! `flare::<area>::<kind>` code.

use super::Ctx;
use super::edit::read_body;
use crate::metrics::Event;
use crate::problem::{Code, Problem};
use crate::reply::Reply;
use crate::schemas::Schemas;
use crate::sharing::{Api, Call, Refusal, Scope, list_after, search_call};
use crate::sign_in::HostedSignIn;
use flare_auth_oidc::Clock as OidcClock;
use flare_contract::{ApiCode, ExchangeRequest, ProblemDocument};
use flare_core::{Credential, PageStore, RecordStore, Timestamp};
use http::StatusCode;
use hyper::body::Incoming;
use serde::de::DeserializeOwned;
use std::time::UNIX_EPOCH;

mod caller;
mod file;
mod login;
mod multipart;
mod query;
mod upload;

use caller::caller;
use login::cli_login;
use query::{every, flag, number, version};

/// The share API's routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApiRoute {
    /// `POST /api/shares`.
    CreateShare,
    /// `GET /api/shares`.
    ListShares,
    /// `GET /api/shares/{id}`, with `?version=n` for a version other than
    /// the latest.
    OpenShare,
    /// `POST /api/shares/{id}/versions`.
    PublishVersion,
    /// `GET /api/shares/{id}/versions/{n}/files/{file}`.
    ShareFile,
    /// `POST /api/grants/{id}`.
    Grant,
    /// `GET /api/grants/{id}`.
    ListGrants,
    /// `DELETE /api/grants/{id}`.
    RevokeGrants,
    /// `POST /api/teams`.
    CreateTeam,
    /// `GET /api/teams`.
    ListTeams,
    /// `GET /api/teams/{name}`.
    ShowTeam,
    /// `DELETE /api/teams/{name}`.
    DeleteTeam,
    /// `PUT /api/teams/{name}/owner`.
    TransferTeam,
    /// `POST /api/teams/{name}/members`.
    AddMembers,
    /// `DELETE /api/teams/{name}/members`.
    RemoveMembers,
    /// `GET /api/activity`.
    Activity,
    /// `GET /api/comments`.
    Comments,
    /// `POST /api/comments`.
    PostComment,
    /// `PATCH /api/comments/{id}`.
    EditComment,
    /// `DELETE /api/comments/{id}`.
    DeleteComment,
    /// `GET /api/cursors/{name}`.
    Cursor,
    /// `PUT /api/cursors/{name}`.
    SaveCursor,
    /// `POST /api/tokens/exchange`.
    Exchange,
    /// `DELETE /api/tokens/{id}`.
    RevokeToken,
    /// `GET /cli/login`.
    CliLogin,
    /// `GET /api/search`.
    Search,
    /// `GET /api/search/fields`.
    SearchFields,
}

/// The most entries a share list holds, and its default.
const MAX_LIST: usize = 500;
const DEFAULT_LIST: usize = 50;

/// The problem document of `refusal` as a reply.
fn refused(refusal: Refusal, origin: &str) -> Reply {
    let mut document = ProblemDocument::new(refusal.code, origin);
    if let Some(detail) = refusal.detail {
        document = document.with_detail(detail);
    }
    if let Some(share) = refusal.share {
        document = document.with_share(share);
    }
    document = document.with_errors(refusal.errors);
    let status = StatusCode::from_u16(refusal.code.status()).unwrap_or(StatusCode::BAD_REQUEST);
    let body = serde_json::to_string(&document).unwrap_or_default();
    Reply::with_body(status, "application/problem+json", body)
}

/// The server's wall-clock time as a [`Timestamp`].
fn timestamp<P, R, S, K: OidcClock, H: HostedSignIn<K>>(ctx: &Ctx<'_, P, R, S, K, H>) -> Timestamp {
    let seconds = ctx
        .shared
        .clock
        .wall()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    Timestamp::from_secs(seconds)
}

/// Reads and parses a JSON body.
pub(super) async fn body_of<T, P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    body: Incoming,
) -> Result<Result<T, Refusal>, Problem>
where
    T: DeserializeOwned,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let bytes = read_body(ctx, body).await?;
    Ok(serde_json::from_slice(&bytes).map_err(|error| Refusal {
        code: ApiCode::BadBody,
        detail: Some(format!("line {}, column {}", error.line(), error.column())),
        share: None,
        errors: Vec::new(),
    }))
}

/// Answers one route of the share API, and counts its token event from
/// the final answer's status.
pub(crate) async fn handle<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    route: ApiRoute,
    name: &str,
    credential: Option<Credential>,
    body: Incoming,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let Some(api) = ctx.shared.sharing.as_deref() else {
        return Err(Code::NotFound.into());
    };
    let reply = answer(ctx, api, route, name, credential, body)
        .await
        .unwrap_or_else(|problem| problem.reply(ctx.headers));
    if let Some(event) = token_event(route, reply.status()) {
        ctx.shared.count(event);
    }
    Ok(reply)
}

/// The token event an answer of `route` with `status` counts: an exchange
/// that answered 2xx traded a code, one that answered 4xx refused it, and
/// any other `/api/` route that answered 401 refused the request's
/// credential. A 5xx, such as a token store that is down, counts nothing.
fn token_event(route: ApiRoute, status: StatusCode) -> Option<Event> {
    match route {
        ApiRoute::Exchange if status.is_success() => Some(Event::TokenExchanged),
        ApiRoute::Exchange if status.is_client_error() => Some(Event::TokenRefused),
        ApiRoute::Exchange | ApiRoute::CliLogin => None,
        _ => (status == StatusCode::UNAUTHORIZED).then_some(Event::TokenRefused),
    }
}

/// The answer of one route of the share API from `api`.
async fn answer<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    api: &dyn Api,
    route: ApiRoute,
    name: &str,
    credential: Option<Credential>,
    body: Incoming,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let origin = ctx
        .shared
        .site
        .origin()
        .map_or("", |origin| origin.as_str());
    let now = timestamp(ctx);
    let answer = match route {
        ApiRoute::CliLogin => {
            return Ok(cli_login(ctx, api, credential.as_ref(), now)
                .await
                .unwrap_or_else(|refusal| refused(refusal, origin)));
        }
        ApiRoute::Exchange => match body_of::<ExchangeRequest, _, _, _, _, _>(ctx, body).await? {
            Ok(request) => api.exchange(request, now).await,
            Err(refusal) => Err(refusal),
        },
        ApiRoute::ShareFile => {
            let served = match caller(ctx, api, credential.as_ref(), now).await {
                Ok(who) => file::serve(api, &who, name).await,
                Err(refusal) => Err(refusal),
            };
            return Ok(served.unwrap_or_else(|refusal| refused(refusal, origin)));
        }
        _ => match caller(ctx, api, credential.as_ref(), now).await {
            Err(refusal) => Err(refusal),
            Ok(who) => {
                let answer = match upload::boundary_of(ctx, route) {
                    Some(boundary) => {
                        upload::run(ctx, api, &who, (route, name), &boundary, body, now).await?
                    }
                    None => match call(ctx, route, name, body).await? {
                        Ok(call) => api.run(&who, call, now).await,
                        Err(refusal) => Err(refusal),
                    },
                };
                if route == ApiRoute::CreateShare && answer.is_ok() {
                    ctx.shared.count(Event::ShareCreated);
                }
                answer
            }
        },
    };
    Ok(match answer {
        Ok((status, body)) => {
            let status = StatusCode::from_u16(status).unwrap_or(StatusCode::OK);
            match body {
                Some(body) => Reply::with_body(status, "application/json", body),
                None => Reply::empty(status),
            }
        }
        Err(refusal) => refused(refusal, origin),
    })
}

/// Parses an authenticated route's request.
async fn call<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    route: ApiRoute,
    name: &str,
    body: Incoming,
) -> Result<Result<Call, Refusal>, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    Ok(match route {
        ApiRoute::CreateShare => body_of(ctx, body).await?.map(Call::Create),
        ApiRoute::PublishVersion => body_of(ctx, body)
            .await?
            .map(|request| Call::Publish(name.to_owned(), request)),
        ApiRoute::Grant => body_of(ctx, body)
            .await?
            .map(|request| Call::Grant(name.to_owned(), request)),
        ApiRoute::RevokeGrants => body_of(ctx, body)
            .await?
            .map(|request| Call::RevokeGrants(name.to_owned(), request)),
        ApiRoute::CreateTeam => body_of(ctx, body).await?.map(Call::CreateTeam),
        ApiRoute::AddMembers => body_of(ctx, body)
            .await?
            .map(|request| Call::AddMembers(name.to_owned(), request)),
        ApiRoute::RemoveMembers => body_of(ctx, body)
            .await?
            .map(|request| Call::RemoveMembers(name.to_owned(), request)),
        ApiRoute::ListGrants => Ok(Call::ListGrants(name.to_owned())),
        ApiRoute::ListTeams => Ok(Call::ListTeams),
        ApiRoute::ShowTeam => Ok(Call::ShowTeam(name.to_owned())),
        ApiRoute::DeleteTeam => Ok(Call::DeleteTeam(name.to_owned())),
        ApiRoute::TransferTeam => body_of(ctx, body)
            .await?
            .map(|request| Call::TransferTeam(name.to_owned(), request)),
        ApiRoute::OpenShare => version(ctx).map(|number| Call::Open(name.to_owned(), number)),
        ApiRoute::RevokeToken => Ok(Call::Revoke(name.to_owned())),
        ApiRoute::ListShares => {
            let scope = match ctx.query("scope").as_deref() {
                Some("mine") => Ok(Scope::Mine),
                Some("with-me") => Ok(Scope::WithMe),
                _ => Err(ApiCode::BadQuery.into()),
            };
            let everyone = flag(ctx, "everyone");
            scope.and_then(|scope| {
                let everyone = everyone?;
                let after = list_after(every(ctx.uri, "cursor"), scope)?;
                number(ctx, MAX_LIST, DEFAULT_LIST).map(|limit| Call::List {
                    scope,
                    after,
                    limit,
                    everyone,
                })
            })
        }
        ApiRoute::Activity => {
            let flares = every(ctx.uri, "flare");
            let since = every(ctx.uri, "since");
            let since_last = every(ctx.uri, "since_last");
            if flares.is_empty() || since.len() > 1 || since_last.len() > 1 {
                Err(ApiCode::BadQuery.into())
            } else {
                number(ctx, flare_core::MAX_PAGE, DEFAULT_LIST).map(|limit| Call::Activity {
                    flares,
                    since: since.into_iter().next(),
                    since_last: since_last.into_iter().next(),
                    limit,
                })
            }
        }
        ApiRoute::Comments => ctx
            .query("flare")
            .map(Call::Comments)
            .ok_or_else(|| ApiCode::BadQuery.into()),
        ApiRoute::PostComment => body_of(ctx, body).await?.map(Call::Post),
        ApiRoute::EditComment => body_of(ctx, body)
            .await?
            .map(|request| Call::Edit(name.to_owned(), request)),
        ApiRoute::DeleteComment => Ok(Call::Delete(name.to_owned())),
        ApiRoute::Cursor => Ok(Call::Cursor(name.to_owned())),
        ApiRoute::SaveCursor => body_of(ctx, body)
            .await?
            .map(|request| Call::SaveCursor(name.to_owned(), request)),
        ApiRoute::Search => number(ctx, MAX_LIST, DEFAULT_LIST)
            .and_then(|limit| search_call(every(ctx.uri, "q"), limit)),
        ApiRoute::SearchFields => Ok(Call::SearchFields),
        ApiRoute::Exchange | ApiRoute::CliLogin | ApiRoute::ShareFile => {
            Err(ApiCode::BadQuery.into())
        }
    })
}

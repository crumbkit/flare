//! The share API's routes on the hosted site, and `/cli/login`, which
//! hands a signed-in browser's login code to the `flare` command.
//!
//! Every `/api/` route but the code exchange takes only a bearer token.
//! `hosted::check` never reads a cookie for an `/api/` path, so a page on
//! another site cannot make a browser act through the API, and the API
//! needs no Origin check. A token that is not a flare token goes to the
//! deployment's authenticator, which may accept bearer tokens of its own.
//! Every refusal is an RFC 9457 problem document with a
//! `flare::<area>::<kind>` code.

use super::Ctx;
use super::edit::read_body;
use crate::metrics::Event;
use crate::problem::{Code, Problem};
use crate::reply::Reply;
use crate::schemas::Schemas;
use crate::sharing::{Api, Call, LoginRedirect, Refusal, Scope, TokenCheck, bearer_of};
use flare_auth_oidc::Clock as OidcClock;
use flare_contract::{ApiCode, ExchangeRequest, ProblemDocument};
use flare_core::{Credential, PageStore, Principal, RecordStore, ShareVersion, Timestamp};
use http::StatusCode;
use hyper::body::Incoming;
use serde::de::DeserializeOwned;
use std::time::UNIX_EPOCH;

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
    /// `POST /api/grants/{id}`.
    Grant,
    /// `GET /api/activity`.
    Activity,
    /// `POST /api/tokens/exchange`.
    Exchange,
    /// `DELETE /api/tokens/{id}`.
    RevokeToken,
    /// `GET /cli/login`.
    CliLogin,
}

/// The most entries a share list holds, and its default.
const MAX_LIST: usize = 500;
const DEFAULT_LIST: usize = 50;

/// The shortest and longest state that `flare login` sends.
const STATE_CHARS: std::ops::RangeInclusive<usize> = 16..=128;

/// The problem document of `refusal` as a reply.
fn refused(refusal: Refusal, origin: &str) -> Reply {
    let mut document = ProblemDocument::new(refusal.code, origin);
    if let Some(detail) = refusal.detail {
        document = document.with_detail(detail);
    }
    if let Some(share) = refusal.share {
        document = document.with_share(share);
    }
    let status = StatusCode::from_u16(refusal.code.status()).unwrap_or(StatusCode::BAD_REQUEST);
    let body = serde_json::to_string(&document).unwrap_or_default();
    Reply::with_body(status, "application/problem+json", body)
}

/// The server's wall-clock time as a [`Timestamp`].
fn timestamp<P, R, S, K: OidcClock>(ctx: &Ctx<'_, P, R, S, K>) -> Timestamp {
    let seconds = ctx
        .shared
        .clock
        .wall()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    Timestamp::from_secs(seconds)
}

/// A number in the query between 1 and `max`, or `default` when absent.
fn number<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    max: usize,
    default: usize,
) -> Result<usize, Refusal>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    match ctx.query("limit") {
        None => Ok(default),
        Some(text) => text
            .parse::<usize>()
            .ok()
            .filter(|value| (1..=max).contains(value))
            .ok_or_else(|| ApiCode::BadQuery.into()),
    }
}

/// The share version that `version` in the query names, or `None` when
/// absent. A value that is not a number from 1 is a bad query.
fn version<P, R, S, K>(ctx: &Ctx<'_, P, R, S, K>) -> Result<Option<ShareVersion>, Refusal>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    match ctx.query("version") {
        None => Ok(None),
        Some(text) => text
            .parse::<u32>()
            .ok()
            .and_then(|number| ShareVersion::try_from(number).ok())
            .map(Some)
            .ok_or_else(|| ApiCode::BadQuery.into()),
    }
}

/// Every value of `name` in the query, decoded.
fn every(ctx_uri: &http::Uri, name: &str) -> Vec<String> {
    let query = ctx_uri.query().unwrap_or_default();
    form_urlencoded::parse(query.as_bytes())
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .collect()
}

/// Reads and parses a JSON body.
async fn body_of<T, P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    body: Incoming,
) -> Result<Result<T, Refusal>, Problem>
where
    T: DeserializeOwned,
    K: OidcClock,
{
    let bytes = read_body(ctx, body).await?;
    Ok(serde_json::from_slice(&bytes).map_err(|error| Refusal {
        code: ApiCode::BadBody,
        detail: Some(format!("line {}, column {}", error.line(), error.column())),
        share: None,
    }))
}

/// The caller that the request's bearer token proves.
async fn caller<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    api: &dyn Api,
    credential: Option<&Credential>,
    now: Timestamp,
) -> Result<Principal, Refusal>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    let credential = credential.ok_or(ApiCode::SignInNeeded)?;
    let token = bearer_of(credential).ok_or(ApiCode::SignInNeeded)?;
    match api.authenticate(token, now).await {
        TokenCheck::Principal(principal) => Ok(principal),
        TokenCheck::Refused(code) => Err(code.into()),
        TokenCheck::NotOurs => match ctx.shared.site.authenticate(credential).await {
            Ok(admitted) => Ok(admitted.principal),
            Err(flare_core::AuthError::Rejected) => Err(ApiCode::SignInNeeded.into()),
            Err(_) => Err(ApiCode::TokensUnavailable.into()),
        },
    }
}

/// Answers one route of the share API.
pub(crate) async fn handle<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
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
{
    let Some(api) = ctx.shared.sharing.as_deref() else {
        return Err(Code::NotFound.into());
    };
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
        ApiRoute::Exchange => {
            let answer = match body_of::<ExchangeRequest, _, _, _, _>(ctx, body).await? {
                Ok(request) => api.exchange(request, now).await,
                Err(refusal) => Err(refusal),
            };
            ctx.shared.count(match &answer {
                Ok(_) => Event::TokenExchanged,
                Err(_) => Event::TokenRefused,
            });
            answer
        }
        _ => match caller(ctx, api, credential.as_ref(), now).await {
            Err(refusal) => {
                if refusal.code.status() == 401 {
                    ctx.shared.count(Event::TokenRefused);
                }
                Err(refusal)
            }
            Ok(who) => match call(ctx, route, name, body).await? {
                Ok(call) => {
                    let answer = api.run(&who, call, now).await;
                    if route == ApiRoute::CreateShare && answer.is_ok() {
                        ctx.shared.count(Event::ShareCreated);
                    }
                    answer
                }
                Err(refusal) => Err(refusal),
            },
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
async fn call<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    route: ApiRoute,
    name: &str,
    body: Incoming,
) -> Result<Result<Call, Refusal>, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    Ok(match route {
        ApiRoute::CreateShare => body_of(ctx, body).await?.map(Call::Create),
        ApiRoute::PublishVersion => body_of(ctx, body)
            .await?
            .map(|request| Call::Publish(name.to_owned(), request)),
        ApiRoute::Grant => body_of(ctx, body)
            .await?
            .map(|request| Call::Grant(name.to_owned(), request)),
        ApiRoute::OpenShare => version(ctx).map(|number| Call::Open(name.to_owned(), number)),
        ApiRoute::RevokeToken => Ok(Call::Revoke(name.to_owned())),
        ApiRoute::ListShares => {
            let scope = match ctx.query("scope").as_deref() {
                Some("mine") => Ok(Scope::Mine),
                Some("with-me") => Ok(Scope::WithMe),
                _ => Err(ApiCode::BadQuery.into()),
            };
            scope.and_then(|scope| {
                number(ctx, MAX_LIST, DEFAULT_LIST).map(|limit| Call::List { scope, limit })
            })
        }
        ApiRoute::Activity => {
            let flares = every(ctx.uri, "flare");
            let since = every(ctx.uri, "since");
            if flares.is_empty() || since.len() > 1 {
                Err(ApiCode::BadQuery.into())
            } else {
                number(ctx, flare_core::MAX_PAGE, DEFAULT_LIST).map(|limit| Call::Activity {
                    flares,
                    since: since.into_iter().next(),
                    limit,
                })
            }
        }
        ApiRoute::Exchange | ApiRoute::CliLogin => Err(ApiCode::BadQuery.into()),
    })
}

/// `GET /cli/login?port=<n>&state=<s>`: a signed-in browser gets a 303 to
/// the CLI's loopback listener with a login code; a browser with no
/// session is sent to sign in and comes back here.
async fn cli_login<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    api: &dyn Api,
    credential: Option<&Credential>,
    now: Timestamp,
) -> Result<Reply, Refusal>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    let port = ctx
        .query("port")
        .and_then(|text| text.parse::<u16>().ok())
        .filter(|port| *port != 0)
        .ok_or(ApiCode::BadQuery)?;
    let state = ctx
        .query("state")
        .filter(|state| {
            STATE_CHARS.contains(&state.len())
                && state
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
        .ok_or(ApiCode::BadQuery)?;
    let signed_in = match credential {
        Some(credential @ Credential::Session(_)) => {
            ctx.shared.site.authenticate(credential).await.ok()
        }
        _ => None,
    };
    let Some(email) = signed_in.and_then(|admitted| admitted.principal.email().cloned()) else {
        let back = format!("/cli/login?port={port}&state={state}");
        let encoded: String = form_urlencoded::byte_serialize(back.as_bytes()).collect();
        return Ok(Reply::see_other(format!(
            "/auth/oidc/start?return_to={encoded}"
        )));
    };
    let code = api.issue_code(&email, port, now).await?;
    Ok(Reply::see_other(
        LoginRedirect::new(port, state, code).location(),
    ))
}

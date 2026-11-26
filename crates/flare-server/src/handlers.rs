//! Every request's path through the server: the listener's checks, the
//! route table of the request's site, the credential and the handler. Each
//! handler answers a [`Reply`] or a [`Problem`], and `handle` turns both
//! into one reply.

mod api;
mod edit;
mod index;
mod oidc;
mod read;

pub(crate) use api::ApiRoute;
mod view;

use crate::app::{Kind, Shared};
use crate::auth::Admitted;
use crate::checks;
use crate::docs;
use crate::hosted;
use crate::launch;
use crate::metrics::Event;
use crate::problem::{Code, Help, Problem};
use crate::reply::Reply;
use crate::routes::{self, Found, Handler, Need, Route};
use crate::schemas::Schemas;
use crate::sign_in::{HostedRefusal, HostedSignIn, Unsigned};
use crate::site::{Site, SiteKind};
use flare_auth_loopback::Nonce;
use flare_auth_oidc::Clock as OidcClock;
use flare_core::{
    Access, AuthError, Credential, Decision, PageStore, PageStoreError, RecordStore, Resource,
};
use http::{HeaderMap, Method, StatusCode, Uri};
use hyper::Request;
use hyper::body::Incoming;
use std::net::IpAddr;
use std::time::Instant;

/// What one request carries into its handler.
pub(crate) struct Ctx<'a, P, R, S, K: OidcClock, H: HostedSignIn<K>> {
    pub(crate) shared: &'a Shared<P, R, S, K, H>,
    pub(crate) headers: &'a HeaderMap,
    pub(crate) uri: &'a Uri,
    pub(crate) now: Instant,
}

impl<P, R, S, K, H> Ctx<'_, P, R, S, K, H>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    /// The one value of `name` in the query, decoded.
    pub(crate) fn query(&self, name: &str) -> Option<String> {
        checks::query_value(self.uri, name)
    }

    /// Asks the side that admitted the request whether it may do `access`
    /// to `resource`.
    pub(crate) async fn allow(
        &self,
        admitted: &Admitted,
        access: Access,
        resource: &Resource,
    ) -> Result<(), Problem> {
        match self.shared.site.authorize(admitted, access, resource).await {
            Ok(Decision::Allowed) => Ok(()),
            Ok(_) => Err(Code::Denied.into()),
            Err(AuthError::Rejected) => Err(self.sign_in_needed()),
            Err(_) => Err(Code::AuthUnavailable.into()),
        }
    }

    /// The 401 problem, with the sign-in link when there is one: the
    /// OpenID Connect start link when it is on, or the hosted sign-in's
    /// address. The local site also says to run `flare open`.
    pub(crate) fn sign_in_needed(&self) -> Problem {
        let site = &self.shared.site;
        if let Some(Unsigned::SignInAt(at)) = site.hosted_sign_in().map(HostedSignIn::unsigned) {
            let help = Help::ProxySignIn(at.map(str::to_owned));
            return Problem::with_help(Code::SignInNeeded, help);
        }
        let start = self.shared.oidc_origin().map(|origin| {
            let back = if self.uri.path().starts_with('/') {
                self.uri.path()
            } else {
                "/flare/"
            };
            let encoded: String = form_urlencoded::byte_serialize(back.as_bytes()).collect();
            format!("{origin}/auth/oidc/start?return_to={encoded}")
        });
        let help = match (site.kind(), start) {
            (SiteKind::Hosted, Some(start)) => Help::HostedSignIn(start),
            (_, start) => Help::SignIn(start),
        };
        Problem::with_help(Code::SignInNeeded, help)
    }

    /// The problem of a credential that the site's sign-in refused. A
    /// refused check is logged by name and answers with no sign-in link, so
    /// a misconfigured issuer or audience cannot loop the browser.
    pub(crate) fn refused(&self, refusal: HostedRefusal) -> Problem {
        match refusal {
            HostedRefusal::Refused { check } => {
                tracing::info!(check, "the sign-in refused a credential");
                Code::ProxyRefused.into()
            }
            HostedRefusal::NotAdmitted => {
                let out = self
                    .shared
                    .site
                    .hosted_sign_in()
                    .and_then(HostedSignIn::sign_out);
                Problem::with_help(Code::NotAdmitted, Help::SignOut(out.map(str::to_owned)))
            }
            HostedRefusal::Unavailable => Code::AuthUnavailable.into(),
            _ => self.sign_in_needed(),
        }
    }
}

/// The page store's refusal as a problem.
pub(crate) fn store_problem(error: &PageStoreError, not_found: Code) -> Problem {
    match error {
        PageStoreError::NotFound => not_found.into(),
        _ => Code::BoardUnavailable.into(),
    }
}

/// Answers one request, and names the route pattern for the log.
pub(crate) async fn handle<P, R, S, K, H>(
    shared: &Shared<P, R, S, K, H>,
    kind: Kind,
    peer: IpAddr,
    request: Request<Incoming>,
) -> (Reply, &'static str)
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let (parts, body) = request.into_parts();
    let ctx = Ctx {
        shared,
        headers: &parts.headers,
        uri: &parts.uri,
        now: shared.clock.now(),
    };
    if checks::header_bytes(&parts.headers) > shared.limits.header_bytes {
        return (
            Problem::from(Code::HeadersTooLarge).reply(&parts.headers),
            "-",
        );
    }
    let (answer, pattern) = match (kind, &shared.site) {
        (Kind::Oidc { port }, Site::Local { .. }) => {
            (oidc::handle(&ctx, port, peer, &parts.method).await, "oidc")
        }
        (Kind::Launch, Site::Local { .. }) => launch_host(&ctx, peer, &parts.method, body).await,
        (Kind::Hosted, Site::Hosted { origin, sign_in }) => {
            let session = sign_in.cookie();
            hosted_site(&ctx, (origin, session), &parts.method, body).await
        }
        _ => (Err(Code::NotFound.into()), "-"),
    };
    let reply = answer.unwrap_or_else(|problem| problem.reply(&parts.headers));
    (reply, pattern)
}

async fn launch_host<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    peer: IpAddr,
    method: &Method,
    body: Incoming,
) -> (Result<Reply, Problem>, &'static str)
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let Some(gate) = ctx.shared.site.gate() else {
        return (Err(Code::NotFound.into()), "-");
    };
    let checked = match gate.loopback().check(peer, method, ctx.headers) {
        Ok(checked) => checked,
        Err(refusal) => return (Err(checks::refusal(refusal).into()), "-"),
    };
    routed(
        ctx,
        method,
        checked.into_credential(),
        body,
        SiteKind::Local,
    )
    .await
}

/// The hosted site: `hosted::check`, then the same cross-site rule, route
/// table and dispatch as the launch host, among the hosted site's routes.
async fn hosted_site<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    (origin, session): (&crate::site::PublicOrigin, &str),
    method: &Method,
    body: Incoming,
) -> (Result<Reply, Problem>, &'static str)
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    match hosted::check(origin, session, method, ctx.uri.path(), ctx.headers) {
        Ok(credential) => routed(ctx, method, credential, body, SiteKind::Hosted).await,
        Err(code) => (Err(code.into()), "-"),
    }
}

/// The cross-site rule, the route among those `site` serves, the
/// credential the route needs and the handler.
async fn routed<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    method: &Method,
    credential: Option<Credential>,
    body: Incoming,
    site: SiteKind,
) -> (Result<Reply, Problem>, &'static str)
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    if let Err(code) = checks::cross_site(ctx.headers, credential.is_some()) {
        return (Err(code.into()), "-");
    }
    let (route, name) = match routes::find(method, ctx.uri.path(), site) {
        Found::Route(route, name) => (route, name),
        Found::MethodNotAllowed => return (Err(Code::MethodNotAllowed.into()), "-"),
        Found::Nothing => return (Err(Code::NotFound.into()), "-"),
    };
    let answer = match admit(ctx, route.credential, credential.as_ref()).await {
        Ok(admitted) => dispatch(ctx, route, name, admitted, credential, body).await,
        Err(problem) => Err(problem),
    };
    (answer, route.pattern)
}

/// Checks the credential a route needs, and returns the side that accepted
/// it. Nothing keeps the answer past the request.
async fn admit<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    need: Need,
    credential: Option<&Credential>,
) -> Result<Option<Admitted>, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    if matches!(need, Need::Nothing | Need::Token | Need::TokenOrCookie) {
        return Ok(None);
    }
    let credential = credential.ok_or_else(|| ctx.sign_in_needed())?;
    let admitted = ctx
        .shared
        .site
        .sign_in(credential)
        .await
        .map_err(|refusal| ctx.refused(refusal))?;
    match (need, credential) {
        (Need::Session, Credential::Bearer(_)) => Err(Code::NeedsSession.into()),
        (Need::Bearer, Credential::Session(_)) => Err(Code::NeedsBearer.into()),
        _ => Ok(Some(admitted)),
    }
}

async fn dispatch<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    route: &Route,
    name: &str,
    admitted: Option<Admitted>,
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
    let access = route.access.unwrap_or(Access::Read);
    match route.handler {
        Handler::OidcStart => return oidc::hosted_start(ctx).await,
        Handler::OidcCallback => return oidc::hosted_callback(ctx, credential.as_ref()).await,
        Handler::Api(api_route) => {
            return api::handle(ctx, api_route, name, credential, body).await;
        }
        _ => {}
    }
    let signed_in = admitted.map(|admitted| (admitted, credential));
    match (route.handler, signed_in) {
        (Handler::Up, _) => Ok(Reply::empty(StatusCode::OK)),
        (Handler::Ready, _) => index::ready(ctx).await,
        (Handler::Style, _) => Ok(Reply::with_body(
            StatusCode::OK,
            "text/css; charset=utf-8",
            docs::stylesheet(),
        )),
        (Handler::Asset, _) => crate::assets::serve(name),
        (Handler::Read, Some((admitted, _))) => read::page(ctx, &admitted, name).await,
        (Handler::Probe, _) => probe(ctx),
        (Handler::Launch, _) => launch::launch(ctx),
        (Handler::Finish, _) => launch::finish(ctx).await,
        (Handler::Root, Some(_)) => Ok(Reply::see_other("/flare/".to_owned())),
        (Handler::Index, Some((admitted, _))) => index::index(ctx, &admitted, access).await,
        (Handler::Page, Some((admitted, _))) => {
            let answer = view::page(ctx, &admitted, access, name).await;
            if answer.is_ok() {
                ctx.shared.count(Event::PageViewed);
            }
            answer
        }
        (Handler::Binding, Some((admitted, credential))) => {
            view::binding(ctx, (&admitted, access), credential.as_ref(), name).await
        }
        (Handler::Edit, Some((admitted, Some(credential)))) => {
            edit::post(ctx, (&admitted, access), &credential, name, body).await
        }
        (Handler::Codes, Some(_)) => launch::codes(ctx),
        _ => Err(ctx.sign_in_needed()),
    }
}

fn probe<P, R, S, K, H>(ctx: &Ctx<'_, P, R, S, K, H>) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let text = ctx.query("nonce").ok_or(Code::BadQuery)?;
    let nonce = Nonce::try_from(text.as_str()).map_err(|_| Code::BadQuery)?;
    let gate = ctx.shared.site.gate().ok_or(Code::NotFound)?;
    let proof = gate.loopback().prove(&nonce);
    Ok(Reply::with_body(
        StatusCode::OK,
        "text/plain; charset=utf-8",
        proof,
    ))
}

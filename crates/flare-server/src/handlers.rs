//! Every request's path through the server: the listener's checks, the
//! route table of the request's site, the credential and the handler. Each
//! handler answers a [`Reply`] or a [`Problem`], and `handle` turns both
//! into one reply.

mod api;
mod edit;
mod oidc;

pub(crate) use api::ApiRoute;
mod view;

use crate::app::{Kind, Shared};
use crate::auth::Admitted;
use crate::checks;
use crate::docs::{self, Entry};
use crate::hosted;
use crate::launch;
use crate::problem::{Code, Help, Problem};
use crate::reply::Reply;
use crate::routes::{self, Found, Handler, Need, Route};
use crate::schemas::Schemas;
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
pub(crate) struct Ctx<'a, P, R, S, K: OidcClock> {
    pub(crate) shared: &'a Shared<P, R, S, K>,
    pub(crate) headers: &'a HeaderMap,
    pub(crate) uri: &'a Uri,
    pub(crate) now: Instant,
}

impl<P, R, S, K> Ctx<'_, P, R, S, K>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
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

    /// The 401 problem, with the OpenID Connect start link when it is on.
    /// The local site also says to run `flare open`; the hosted site has
    /// only the link.
    pub(crate) fn sign_in_needed(&self) -> Problem {
        let start = self.shared.oidc_origin().map(|origin| {
            let back = if self.uri.path().starts_with('/') {
                self.uri.path()
            } else {
                "/flare/"
            };
            let encoded: String = form_urlencoded::byte_serialize(back.as_bytes()).collect();
            format!("{origin}/auth/oidc/start?return_to={encoded}")
        });
        let help = match (self.shared.site.kind(), start) {
            (SiteKind::Hosted, Some(start)) => Help::HostedSignIn(start),
            (_, start) => Help::SignIn(start),
        };
        Problem::with_help(Code::SignInNeeded, help)
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
pub(crate) async fn handle<P, R, S, K>(
    shared: &Shared<P, R, S, K>,
    kind: Kind,
    peer: IpAddr,
    request: Request<Incoming>,
) -> (Reply, &'static str)
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
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
        (Kind::Hosted, Site::Hosted { origin, .. }) => {
            hosted_site(&ctx, origin, &parts.method, body).await
        }
        _ => (Err(Code::NotFound.into()), "-"),
    };
    let reply = answer.unwrap_or_else(|problem| problem.reply(&parts.headers));
    (reply, pattern)
}

async fn launch_host<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    peer: IpAddr,
    method: &Method,
    body: Incoming,
) -> (Result<Reply, Problem>, &'static str)
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
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
async fn hosted_site<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    origin: &crate::site::PublicOrigin,
    method: &Method,
    body: Incoming,
) -> (Result<Reply, Problem>, &'static str)
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    match hosted::check(origin, method, ctx.uri.path(), ctx.headers) {
        Ok(credential) => routed(ctx, method, credential, body, SiteKind::Hosted).await,
        Err(code) => (Err(code.into()), "-"),
    }
}

/// The cross-site rule, the route among those `site` serves, the
/// credential the route needs and the handler.
async fn routed<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
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
async fn admit<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    need: Need,
    credential: Option<&Credential>,
) -> Result<Option<Admitted>, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    if matches!(need, Need::Nothing | Need::Token) {
        return Ok(None);
    }
    let credential = credential.ok_or_else(|| ctx.sign_in_needed())?;
    let admitted = match ctx.shared.site.authenticate(credential).await {
        Ok(admitted) => admitted,
        Err(AuthError::Rejected) => return Err(ctx.sign_in_needed()),
        Err(_) => return Err(Code::AuthUnavailable.into()),
    };
    match (need, credential) {
        (Need::Session, Credential::Bearer(_)) => Err(Code::NeedsSession.into()),
        (Need::Bearer, Credential::Session(_)) => Err(Code::NeedsBearer.into()),
        _ => Ok(Some(admitted)),
    }
}

async fn dispatch<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
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
{
    let access = route.access.unwrap_or(Access::Read);
    match route.handler {
        Handler::OidcStart => return oidc::hosted_start(ctx),
        Handler::OidcCallback => return oidc::hosted_callback(ctx, credential.as_ref()).await,
        Handler::Api(api_route) => {
            return api::handle(ctx, api_route, name, credential, body).await;
        }
        _ => {}
    }
    let signed_in = admitted.map(|admitted| (admitted, credential));
    match (route.handler, signed_in) {
        (Handler::Up, _) => Ok(Reply::empty(StatusCode::OK)),
        (Handler::Ready, _) => ready(ctx).await,
        (Handler::Style, _) => Ok(Reply::with_body(
            StatusCode::OK,
            "text/css; charset=utf-8",
            docs::stylesheet(),
        )),
        (Handler::Probe, _) => probe(ctx),
        (Handler::Launch, _) => launch::launch(ctx),
        (Handler::Finish, _) => launch::finish(ctx),
        (Handler::Root, Some(_)) => Ok(Reply::see_other("/flare/".to_owned())),
        (Handler::Index, Some((admitted, _))) => index(ctx, &admitted, access).await,
        (Handler::Page, Some((admitted, _))) => view::page(ctx, &admitted, access, name).await,
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

async fn ready<P, R, S, K>(ctx: &Ctx<'_, P, R, S, K>) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    match ctx.shared.pages.pages().await {
        Ok(_) => Ok(Reply::empty(StatusCode::OK)),
        Err(_) => Err(Code::BoardUnavailable.into()),
    }
}

fn probe<P, R, S, K>(ctx: &Ctx<'_, P, R, S, K>) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
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

async fn index<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    admitted: &Admitted,
    access: Access,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    let pages = &ctx.shared.pages;
    let names = pages
        .pages()
        .await
        .map_err(|error| store_problem(&error, Code::PageNotFound))?;
    let mut entries = Vec::new();
    for name in names {
        let resource = Resource::Page(name.clone());
        if ctx.allow(admitted, access, &resource).await.is_err() {
            continue;
        }
        let mut bindings = Vec::new();
        // Bindings are a local-only feature, so the hosted index lists
        // pages alone.
        let listed = match ctx.shared.site.kind() {
            SiteKind::Local => pages
                .bindings_of(&name)
                .await
                .map_err(|error| store_problem(&error, Code::PageNotFound))?,
            SiteKind::Hosted => Vec::new(),
        };
        for binding in listed {
            let resource = Resource::Binding(binding.name().clone());
            if ctx.allow(admitted, access, &resource).await.is_ok() {
                bindings.push(binding.name().as_str().to_owned());
            }
        }
        entries.push(Entry {
            page: name.as_str().to_owned(),
            bindings,
        });
    }
    Ok(Reply::html(StatusCode::OK, docs::index(&entries)))
}

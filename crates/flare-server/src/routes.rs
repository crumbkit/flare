//! The route table of the launch host and the hosted site: one static
//! array, so the tests can walk it and see that no GET route writes and
//! that no writing route serves the hosted site. Each route names the
//! sites that serve it. The local sign-in host's two routes are in
//! `handlers/oidc.rs`.

use crate::handlers::ApiRoute;
use crate::site::SiteKind;
use flare_core::Access;
use http::Method;

/// The sites that serve a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sites {
    /// The local site's launch host only.
    Local,
    /// The hosted site only.
    Hosted,
    /// Both.
    Both,
}

impl Sites {
    /// Whether `site` serves a route with these sites.
    pub(crate) fn serves(self, site: SiteKind) -> bool {
        matches!(
            (self, site),
            (Self::Both, _) | (Self::Local, SiteKind::Local) | (Self::Hosted, SiteKind::Hosted)
        )
    }
}

/// The credential a route needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Need {
    /// None: the route is open.
    Nothing,
    /// A bearer token or a session.
    Credential,
    /// A session cookie; a bearer token is refused.
    Session,
    /// The bearer token; a session is refused.
    Bearer,
    /// A bearer token that the share API checks itself; the hosted site
    /// reads no cookie for these routes.
    Token,
}

/// What a route does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Handler {
    Up,
    Ready,
    Launch,
    Finish,
    Style,
    Asset,
    Read,
    Root,
    Index,
    Page,
    Binding,
    Edit,
    Codes,
    Probe,
    OidcStart,
    OidcCallback,
    Api(ApiRoute),
}

#[cfg(test)]
impl Handler {
    /// Whether the handler writes a record, a page or a binding.
    pub(crate) fn writes(self) -> bool {
        self == Self::Edit
    }
}

/// One route.
#[derive(Debug)]
pub(crate) struct Route {
    /// The method.
    pub(crate) method: Method,
    /// The path, or with `<name>` the prefix before the one name segment.
    pub(crate) pattern: &'static str,
    /// The credential it needs.
    pub(crate) credential: Need,
    /// The access its handler asks `authorize` for, on each resource it
    /// reads or writes.
    pub(crate) access: Option<Access>,
    /// What it does.
    pub(crate) handler: Handler,
    /// The sites that serve it.
    pub(crate) sites: Sites,
}

const fn route(
    method: Method,
    pattern: &'static str,
    (credential, access): (Need, Option<Access>),
    handler: Handler,
    sites: Sites,
) -> Route {
    Route {
        method,
        pattern,
        credential,
        access,
        handler,
        sites,
    }
}

const OPEN: (Need, Option<Access>) = (Need::Nothing, None);
const READ: (Need, Option<Access>) = (Need::Credential, Some(Access::Read));
const TOKEN: (Need, Option<Access>) = (Need::Token, None);

/// The table.
pub(crate) static ROUTES: [Route; 24] = [
    route(Method::GET, "/up", OPEN, Handler::Up, Sites::Both),
    route(Method::GET, "/ready", OPEN, Handler::Ready, Sites::Local),
    route(Method::GET, "/launch", OPEN, Handler::Launch, Sites::Local),
    route(
        Method::GET,
        "/auth/oidc/finish",
        OPEN,
        Handler::Finish,
        Sites::Local,
    ),
    route(
        Method::GET,
        "/auth/oidc/start",
        OPEN,
        Handler::OidcStart,
        Sites::Hosted,
    ),
    route(
        Method::GET,
        "/auth/oidc/callback",
        OPEN,
        Handler::OidcCallback,
        Sites::Hosted,
    ),
    route(
        Method::GET,
        "/flare/style.css",
        OPEN,
        Handler::Style,
        Sites::Both,
    ),
    route(
        Method::GET,
        "/flare/assets/<name>",
        OPEN,
        Handler::Asset,
        Sites::Both,
    ),
    route(
        Method::GET,
        "/",
        (Need::Credential, None),
        Handler::Root,
        Sites::Both,
    ),
    route(
        Method::GET,
        "/s/<name>",
        (Need::Credential, None),
        Handler::Read,
        Sites::Hosted,
    ),
    route(Method::GET, "/flare/", READ, Handler::Index, Sites::Both),
    route(
        Method::GET,
        "/pages/<name>",
        READ,
        Handler::Page,
        Sites::Both,
    ),
    route(
        Method::GET,
        "/b/<name>",
        READ,
        Handler::Binding,
        Sites::Local,
    ),
    route(
        Method::POST,
        "/b/<name>",
        (Need::Session, Some(Access::Write)),
        Handler::Edit,
        Sites::Local,
    ),
    route(
        Method::POST,
        "/launch/codes",
        (Need::Bearer, None),
        Handler::Codes,
        Sites::Local,
    ),
    route(
        Method::GET,
        "/launch/probe",
        OPEN,
        Handler::Probe,
        Sites::Local,
    ),
    route(
        Method::GET,
        "/cli/login",
        OPEN,
        Handler::Api(ApiRoute::CliLogin),
        Sites::Hosted,
    ),
    route(
        Method::POST,
        "/api/tokens/exchange",
        OPEN,
        Handler::Api(ApiRoute::Exchange),
        Sites::Hosted,
    ),
    route(
        Method::DELETE,
        "/api/tokens/<name>",
        TOKEN,
        Handler::Api(ApiRoute::RevokeToken),
        Sites::Hosted,
    ),
    route(
        Method::POST,
        "/api/shares",
        TOKEN,
        Handler::Api(ApiRoute::CreateShare),
        Sites::Hosted,
    ),
    route(
        Method::GET,
        "/api/shares",
        TOKEN,
        Handler::Api(ApiRoute::ListShares),
        Sites::Hosted,
    ),
    route(
        Method::GET,
        "/api/shares/<name>",
        TOKEN,
        Handler::Api(ApiRoute::OpenShare),
        Sites::Hosted,
    ),
    route(
        Method::POST,
        "/api/grants/<name>",
        TOKEN,
        Handler::Api(ApiRoute::Grant),
        Sites::Hosted,
    ),
    route(
        Method::GET,
        "/api/activity",
        TOKEN,
        Handler::Api(ApiRoute::Activity),
        Sites::Hosted,
    ),
];

/// What the table has for a request.
#[derive(Debug)]
pub(crate) enum Found<'p> {
    /// This route, and the name segment when its pattern has one.
    Route(&'static Route, &'p str),
    /// A route has this path and another method.
    MethodNotAllowed,
    /// No route has this path.
    Nothing,
}

/// The text after a pattern's prefix when `path` matches it.
fn matches<'p>(pattern: &str, path: &'p str) -> Option<&'p str> {
    match pattern.strip_suffix("<name>") {
        Some(prefix) => path.strip_prefix(prefix),
        None => (pattern == path).then_some(""),
    }
}

/// The route for `method` and `path` among the routes `site` serves.
pub(crate) fn find<'p>(method: &Method, path: &'p str, site: SiteKind) -> Found<'p> {
    let mut path_known = false;
    for route in ROUTES.iter().filter(|route| route.sites.serves(site)) {
        if let Some(name) = matches(route.pattern, path) {
            if route.method == method {
                return Found::Route(route, name);
            }
            path_known = true;
        }
    }
    if path_known {
        Found::MethodNotAllowed
    } else {
        Found::Nothing
    }
}

/// The method and pattern of every route the local site's launch host
/// serves.
pub(crate) fn listed() -> impl Iterator<Item = (Method, &'static str)> {
    ROUTES
        .iter()
        .filter(|route| route.sites.serves(SiteKind::Local))
        .map(|route| (route.method.clone(), route.pattern))
}

#[cfg(test)]
mod tests;

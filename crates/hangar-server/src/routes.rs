//! The launch host's route table: one static array, so the tests can walk
//! it and see that no GET route writes. The sign-in host's two routes are
//! in `handlers/oidc.rs`.

use hangar_core::Access;
use http::Method;

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
}

/// What a route does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Handler {
    Up,
    Ready,
    Launch,
    Finish,
    Style,
    Root,
    Index,
    Page,
    Binding,
    Edit,
    Codes,
    Probe,
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
}

const fn route(
    method: Method,
    pattern: &'static str,
    credential: Need,
    access: Option<Access>,
    handler: Handler,
) -> Route {
    Route {
        method,
        pattern,
        credential,
        access,
        handler,
    }
}

/// The table.
pub(crate) static ROUTES: [Route; 12] = [
    route(Method::GET, "/up", Need::Nothing, None, Handler::Up),
    route(Method::GET, "/ready", Need::Nothing, None, Handler::Ready),
    route(Method::GET, "/launch", Need::Nothing, None, Handler::Launch),
    route(
        Method::GET,
        "/auth/oidc/finish",
        Need::Nothing,
        None,
        Handler::Finish,
    ),
    route(
        Method::GET,
        "/hangar/style.css",
        Need::Nothing,
        None,
        Handler::Style,
    ),
    route(Method::GET, "/", Need::Credential, None, Handler::Root),
    route(
        Method::GET,
        "/hangar/",
        Need::Credential,
        Some(Access::Read),
        Handler::Index,
    ),
    route(
        Method::GET,
        "/pages/<name>",
        Need::Credential,
        Some(Access::Read),
        Handler::Page,
    ),
    route(
        Method::GET,
        "/b/<name>",
        Need::Credential,
        Some(Access::Read),
        Handler::Binding,
    ),
    route(
        Method::POST,
        "/b/<name>",
        Need::Session,
        Some(Access::Write),
        Handler::Edit,
    ),
    route(
        Method::POST,
        "/launch/codes",
        Need::Bearer,
        None,
        Handler::Codes,
    ),
    route(
        Method::GET,
        "/launch/probe",
        Need::Nothing,
        None,
        Handler::Probe,
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

/// The route for `method` and `path`.
pub(crate) fn find<'p>(method: &Method, path: &'p str) -> Found<'p> {
    let mut path_known = false;
    for route in &ROUTES {
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

/// Every route's method and pattern.
pub(crate) fn listed() -> impl Iterator<Item = (Method, &'static str)> {
    ROUTES
        .iter()
        .map(|route| (route.method.clone(), route.pattern))
}

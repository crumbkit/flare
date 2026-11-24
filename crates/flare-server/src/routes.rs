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
    /// A bearer token that the share API checks itself, or, when the
    /// request has no Authorization header, the session cookie of a page
    /// of this site.
    TokenOrCookie,
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
const BROWSER: (Need, Option<Access>) = (Need::TokenOrCookie, None);

/// The table.
pub(crate) static ROUTES: [Route; 40] = [
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
        "/api/shares/<name>/versions",
        TOKEN,
        Handler::Api(ApiRoute::PublishVersion),
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
        "/api/grants/<name>",
        TOKEN,
        Handler::Api(ApiRoute::ListGrants),
        Sites::Hosted,
    ),
    route(
        Method::DELETE,
        "/api/grants/<name>",
        TOKEN,
        Handler::Api(ApiRoute::RevokeGrants),
        Sites::Hosted,
    ),
    route(
        Method::POST,
        "/api/teams",
        TOKEN,
        Handler::Api(ApiRoute::CreateTeam),
        Sites::Hosted,
    ),
    route(
        Method::GET,
        "/api/teams",
        TOKEN,
        Handler::Api(ApiRoute::ListTeams),
        Sites::Hosted,
    ),
    // The members and owner routes come before the team's own, whose
    // pattern ends in <name> and would otherwise take `design/members` as
    // the name.
    route(
        Method::POST,
        "/api/teams/<name>/members",
        TOKEN,
        Handler::Api(ApiRoute::AddMembers),
        Sites::Hosted,
    ),
    route(
        Method::DELETE,
        "/api/teams/<name>/members",
        TOKEN,
        Handler::Api(ApiRoute::RemoveMembers),
        Sites::Hosted,
    ),
    route(
        Method::PUT,
        "/api/teams/<name>/owner",
        TOKEN,
        Handler::Api(ApiRoute::TransferTeam),
        Sites::Hosted,
    ),
    route(
        Method::GET,
        "/api/teams/<name>",
        TOKEN,
        Handler::Api(ApiRoute::ShowTeam),
        Sites::Hosted,
    ),
    route(
        Method::DELETE,
        "/api/teams/<name>",
        TOKEN,
        Handler::Api(ApiRoute::DeleteTeam),
        Sites::Hosted,
    ),
    route(
        Method::GET,
        "/api/activity",
        BROWSER,
        Handler::Api(ApiRoute::Activity),
        Sites::Hosted,
    ),
    route(
        Method::GET,
        "/api/comments",
        BROWSER,
        Handler::Api(ApiRoute::Comments),
        Sites::Hosted,
    ),
    route(
        Method::POST,
        "/api/comments",
        BROWSER,
        Handler::Api(ApiRoute::PostComment),
        Sites::Hosted,
    ),
    route(
        Method::PATCH,
        "/api/comments/<name>",
        BROWSER,
        Handler::Api(ApiRoute::EditComment),
        Sites::Hosted,
    ),
    route(
        Method::DELETE,
        "/api/comments/<name>",
        BROWSER,
        Handler::Api(ApiRoute::DeleteComment),
        Sites::Hosted,
    ),
    route(
        Method::GET,
        "/api/cursors/<name>",
        TOKEN,
        Handler::Api(ApiRoute::Cursor),
        Sites::Hosted,
    ),
    route(
        Method::PUT,
        "/api/cursors/<name>",
        TOKEN,
        Handler::Api(ApiRoute::SaveCursor),
        Sites::Hosted,
    ),
];

mod find;

pub(crate) use find::{Found, find, listed};

#[cfg(test)]
mod tests;

//! [`Problem`]: every refusal and failure, with its status and code. A
//! client that asks for JSON gets an RFC 9457 problem document, and a
//! browser gets a small hangar document with the same code.

use crate::docs::{self, Band, Doc};
use crate::reply::Reply;
use http::StatusCode;
use http::header::{ACCEPT, HeaderMap};

/// The refusals and failures, one per code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Code {
    NotFound,
    MethodNotAllowed,
    BadName,
    BadQuery,
    Host,
    Peer,
    Origin,
    CrossSite,
    HeadersTooLarge,
    BodyTooLarge,
    Slow,
    BadBody,
    UnsupportedType,
    SignInNeeded,
    BadCredential,
    Denied,
    AuthUnavailable,
    NeedsSession,
    TokenUnknown,
    TokenMismatch,
    BindingChanged,
    BadPost,
    Refused,
    Conflict,
    RecordsBusy,
    RecordNotFound,
    PageNotFound,
    BindingNotFound,
    CannotRender,
    BoardUnavailable,
    LaunchRefused,
    NeedsBearer,
    TooManyCodes,
    LaunchUnavailable,
    OidcRefused,
    OidcUnavailable,
    BadReturn,
    HandoverUnknown,
    ServerBusy,
    Deadline,
    Fault,
}

impl Code {
    /// The status, the code text and the one-line title.
    pub(crate) fn facts(self) -> (StatusCode, &'static str, &'static str) {
        use StatusCode as S;
        match self {
            Self::NotFound => (S::NOT_FOUND, "request::not_found", "Nothing is here."),
            Self::MethodNotAllowed => (
                S::METHOD_NOT_ALLOWED,
                "request::method_not_allowed",
                "This path does not take that method.",
            ),
            Self::BadName => (
                S::BAD_REQUEST,
                "request::bad_name",
                "The path does not name a page or binding: a name is lower-case letters, digits and hyphens.",
            ),
            Self::BadQuery => (
                S::BAD_REQUEST,
                "request::bad_query",
                "The request's query is missing a value or holds a malformed one.",
            ),
            Self::Host => (
                S::FORBIDDEN,
                "request::host",
                "The request's Host header is not this server's.",
            ),
            Self::Peer => (
                S::FORBIDDEN,
                "request::peer",
                "The request did not come from this machine.",
            ),
            Self::Origin => (
                S::FORBIDDEN,
                "request::origin",
                "A request that changes something must come from this server's own pages.",
            ),
            Self::CrossSite => (
                S::FORBIDDEN,
                "request::cross_site",
                "Another site's page asked for this with your sign-in.",
            ),
            Self::HeadersTooLarge => (
                S::REQUEST_HEADER_FIELDS_TOO_LARGE,
                "request::headers_too_large",
                "The request's headers are larger than 32 KiB.",
            ),
            Self::BodyTooLarge => (
                S::PAYLOAD_TOO_LARGE,
                "request::body_too_large",
                "The request's body is larger than 1 MiB.",
            ),
            Self::Slow => (
                S::REQUEST_TIMEOUT,
                "request::slow",
                "The request's body did not arrive in time.",
            ),
            Self::BadBody => (
                S::BAD_REQUEST,
                "request::bad_body",
                "The request's body could not be read.",
            ),
            Self::UnsupportedType => (
                S::UNSUPPORTED_MEDIA_TYPE,
                "request::unsupported_type",
                "A form post must be application/x-www-form-urlencoded.",
            ),
            Self::SignInNeeded => (
                S::UNAUTHORIZED,
                "auth::sign_in_needed",
                "Sign in to see this.",
            ),
            Self::BadCredential => (
                S::UNAUTHORIZED,
                "auth::bad_credential",
                "The request's credential is malformed or repeated.",
            ),
            Self::Denied => (
                S::FORBIDDEN,
                "auth::denied",
                "You are signed in, and this is not yours to reach.",
            ),
            Self::AuthUnavailable => (
                S::SERVICE_UNAVAILABLE,
                "auth::unavailable",
                "The server cannot check your sign-in now.",
            ),
            Self::NeedsSession => (
                S::FORBIDDEN,
                "forms::needs_session",
                "An edit needs a browser session; a bearer token cannot post a form.",
            ),
            Self::TokenUnknown => (
                S::FORBIDDEN,
                "forms::token_unknown",
                "This form is no longer valid. Open the binding again and repeat the edit.",
            ),
            Self::TokenMismatch => (
                S::FORBIDDEN,
                "forms::token_mismatch",
                "This form was drawn for another session, binding, slot or record version.",
            ),
            Self::BindingChanged => (
                S::CONFLICT,
                "forms::binding_changed",
                "The binding changed after this form was drawn. Open it again.",
            ),
            Self::BadPost => (
                S::BAD_REQUEST,
                "forms::bad_post",
                "The post is not one this server's forms send.",
            ),
            Self::Refused => (
                S::UNPROCESSABLE_ENTITY,
                "forms::refused",
                "Your edit was not saved.",
            ),
            Self::Conflict => (
                S::CONFLICT,
                "forms::conflict",
                "The record changed since you opened the form.",
            ),
            Self::RecordsBusy => (
                S::SERVICE_UNAVAILABLE,
                "records::busy",
                "The record kept changing while the edit was written. Try again.",
            ),
            Self::RecordNotFound => (
                S::NOT_FOUND,
                "records::not_found",
                "A record the binding fills is gone.",
            ),
            Self::PageNotFound => (S::NOT_FOUND, "pages::not_found", "No page has this name."),
            Self::BindingNotFound => (
                S::NOT_FOUND,
                "bindings::not_found",
                "No binding has this name.",
            ),
            Self::CannotRender => (
                S::INTERNAL_SERVER_ERROR,
                "pages::cannot_render",
                "The page could not be rendered.",
            ),
            Self::BoardUnavailable => (
                S::SERVICE_UNAVAILABLE,
                "board::unavailable",
                "The board did not answer. Try again.",
            ),
            Self::LaunchRefused => (
                S::FORBIDDEN,
                "launch::code_refused",
                "This launch link is unknown, used or expired. Run hangar open again.",
            ),
            Self::NeedsBearer => (
                S::FORBIDDEN,
                "launch::needs_bearer",
                "Only hangar open, with the server's token, can ask for a launch code.",
            ),
            Self::TooManyCodes => (
                S::SERVICE_UNAVAILABLE,
                "launch::too_many_codes",
                "64 launch codes are outstanding. Try again within a minute.",
            ),
            Self::LaunchUnavailable => (
                S::SERVICE_UNAVAILABLE,
                "launch::unavailable",
                "The server cannot sign you in now.",
            ),
            Self::OidcRefused => (
                S::FORBIDDEN,
                "oidc::refused",
                "The sign-in was refused. Start it again from hangar.",
            ),
            Self::OidcUnavailable => (
                S::SERVICE_UNAVAILABLE,
                "oidc::unavailable",
                "The sign-in provider is unavailable. Try again.",
            ),
            Self::BadReturn => (
                S::BAD_REQUEST,
                "oidc::bad_return",
                "The sign-in must return to a path on this server.",
            ),
            Self::HandoverUnknown => (
                S::FORBIDDEN,
                "oidc::handover_unknown",
                "This sign-in link is unknown, used or expired. Sign in again.",
            ),
            Self::ServerBusy => (
                S::SERVICE_UNAVAILABLE,
                "server::busy",
                "The server is busy. Try again.",
            ),
            Self::Deadline => (
                S::SERVICE_UNAVAILABLE,
                "server::deadline",
                "The request took longer than 30 seconds.",
            ),
            Self::Fault => (
                S::INTERNAL_SERVER_ERROR,
                "server::fault",
                "The server failed while answering. The failure is logged.",
            ),
        }
    }

    /// The status.
    pub(crate) fn status(self) -> StatusCode {
        self.facts().0
    }

    /// The code text, such as `request::not_found`.
    pub(crate) fn text(self) -> &'static str {
        self.facts().1
    }
}

/// What a refusal document offers beside its message.
#[derive(Debug, Clone, Default)]
pub(crate) enum Help {
    /// Nothing more.
    #[default]
    Nothing,
    /// How to sign in: `hangar open`, and with OpenID Connect a link to
    /// this start URL.
    SignIn(Option<String>),
    /// A link back to this local path.
    Back(String),
}

/// A refusal or failure, ready to answer.
#[derive(Debug, Clone)]
pub(crate) struct Problem {
    code: Code,
    help: Help,
}

impl From<Code> for Problem {
    fn from(code: Code) -> Self {
        Self {
            code,
            help: Help::Nothing,
        }
    }
}

impl Problem {
    /// The problem `code` with `help` in its document.
    pub(crate) fn with_help(code: Code, help: Help) -> Self {
        Self { code, help }
    }

    /// The reply: a problem document when the request's `Accept` names
    /// JSON, and a hangar document otherwise.
    pub(crate) fn reply(&self, headers: &HeaderMap) -> Reply {
        let (status, code, title) = self.code.facts();
        if wants_json(headers) {
            let document = serde_json::json!({
                "type": "about:blank",
                "title": title,
                "status": status.as_u16(),
                "code": code,
            });
            return Reply::with_body(status, "application/problem+json", document.to_string());
        }
        let mut main = format!("<div class=\"msg bad\"><b>{}</b>", docs::escape(title));
        match &self.help {
            Help::Nothing => {}
            Help::SignIn(start) => {
                main.push_str("<span>Run <code>hangar open</code> on this Mac to sign in.</span>");
                if let Some(start) = start {
                    main.push_str(&format!(
                        "<span><a href=\"{}\">Sign in with Google</a></span>",
                        docs::escape(start)
                    ));
                }
            }
            Help::Back(path) => {
                main.push_str(&format!(
                    "<span><a href=\"{}\">Open the binding again</a></span>",
                    docs::escape(path)
                ));
            }
        }
        main.push_str(&format!("<span class=\"mono\">{code}</span></div>"));
        Reply::html(
            status,
            Doc::new(title, Band::Hangar).code(code).main(main).finish(),
        )
    }
}

/// Whether the request's `Accept` header names a JSON type.
pub(crate) fn wants_json(headers: &HeaderMap) -> bool {
    headers.get_all(ACCEPT).iter().any(|value| {
        value.to_str().is_ok_and(|text| {
            text.contains("application/json") || text.contains("application/problem+json")
        })
    })
}

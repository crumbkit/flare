//! [`Reply`]: every answer the server gives, and the one function that
//! turns it into a response with the threat model's policy and its six
//! other headers, so no handler can leave one out. Every hosted reply adds
//! `connect-src 'self'`. A reading page that loads a script carries the
//! script policy, which also adds `script-src 'self'`, so its comments
//! section can call the comment routes. An embedded asset, whose name holds
//! its content's hash, is cached for a year.

use crate::app::bytes_alias::Body;
use crate::site::SiteKind;
use http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HeaderName, HeaderValue, LOCATION,
    REFERRER_POLICY, SET_COOKIE, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS,
    X_FRAME_OPTIONS,
};
use http::{Response, StatusCode};
use hyper::body::Bytes;

/// The policy of flare's own documents (threat model, threat 12).
pub(crate) const POLICY: &str = "default-src 'none'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'";

/// The policy of hosted replies: flare's own pages may also open
/// connections to their own origin, which the comment routes need.
pub(crate) const HOSTED_POLICY: &str = "default-src 'none'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'";

/// The policy of a reading page that loads a script: the hosted policy with
/// the page's own scripts allowed. /evals/flare-reading-scripts-under-csp
/// found that Shiki and Mermaid run under it with no violation, and need no
/// `'unsafe-eval'`, worker or `blob:` source. Its `connect-src 'self'` lets
/// the comments section read and write comments on the page's own origin.
pub(crate) const SCRIPT_POLICY: &str = "default-src 'none'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'; script-src 'self'";

/// How long a browser keeps an embedded asset: a year, without asking
/// again, because a new build gives a changed file a new name.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

const CORP: HeaderName = HeaderName::from_static("cross-origin-resource-policy");
const COOP: HeaderName = HeaderName::from_static("cross-origin-opener-policy");

/// The headers every reply carries, beside its policy and its cache rule.
const FIXED: [(HeaderName, &str); 5] = [
    (X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (REFERRER_POLICY, "same-origin"),
    (CORP, "same-origin"),
    (COOP, "same-origin"),
    (X_FRAME_OPTIONS, "DENY"),
];

/// What every hosted reply adds: browsers keep to HTTPS for the origin for
/// two years.
const HSTS: &str = "max-age=63072000";

/// The media type of a flare document.
pub(crate) const HTML: &str = "text/html; charset=utf-8";

/// One answer before it becomes a response.
#[derive(Debug)]
pub(crate) struct Reply {
    status: StatusCode,
    content_type: Option<&'static str>,
    body: Bytes,
    location: Option<String>,
    cookies: Vec<String>,
    scripts: bool,
    immutable: bool,
}

impl Reply {
    /// An empty reply with `status`.
    pub(crate) fn empty(status: StatusCode) -> Self {
        Self {
            status,
            content_type: None,
            body: Bytes::new(),
            location: None,
            cookies: Vec::new(),
            scripts: false,
            immutable: false,
        }
    }

    /// A reply with `body` of `content_type`.
    pub(crate) fn with_body(
        status: StatusCode,
        content_type: &'static str,
        body: impl Into<Bytes>,
    ) -> Self {
        Self {
            content_type: Some(content_type),
            body: body.into(),
            ..Self::empty(status)
        }
    }

    /// A flare document.
    pub(crate) fn html(status: StatusCode, document: String) -> Self {
        Self::with_body(status, HTML, document)
    }

    /// A 303 to `location`, a path on this host or an absolute URL that the
    /// server composed.
    pub(crate) fn see_other(location: String) -> Self {
        Self {
            location: Some(location),
            ..Self::empty(StatusCode::SEE_OTHER)
        }
    }

    /// Lets the page run flare's own scripts: its policy becomes
    /// [`SCRIPT_POLICY`].
    pub(crate) fn with_scripts(mut self) -> Self {
        self.scripts = true;
        self
    }

    /// Marks the reply as an embedded asset that never changes under its
    /// name, so browsers cache it for a year.
    pub(crate) fn immutable(mut self) -> Self {
        self.immutable = true;
        self
    }

    /// Adds a `Set-Cookie` header.
    pub(crate) fn cookie(mut self, value: String) -> Self {
        self.cookies.push(value);
        self
    }

    /// The status.
    pub(crate) fn status(&self) -> StatusCode {
        self.status
    }

    /// The response, with the policy and the six other headers, and on the
    /// hosted site `Strict-Transport-Security`. A location or cookie that
    /// is not a valid header value, which the server never composes, turns
    /// the reply into an empty 500.
    pub(crate) fn into_response(self, site: SiteKind) -> Response<Body> {
        let mut extra = Vec::new();
        if let Some(content_type) = self.content_type {
            extra.push((CONTENT_TYPE, HeaderValue::from_static(content_type)));
        }
        let dynamic = self
            .location
            .into_iter()
            .map(|location| (LOCATION, location))
            .chain(self.cookies.into_iter().map(|cookie| (SET_COOKIE, cookie)));
        for (name, value) in dynamic {
            match HeaderValue::try_from(value) {
                Ok(value) => extra.push((name, value)),
                Err(_) => {
                    return Self::empty(StatusCode::INTERNAL_SERVER_ERROR).into_response(site);
                }
            }
        }
        let mut response = Response::new(Body::new(self.body));
        *response.status_mut() = self.status;
        let headers = response.headers_mut();
        let policy = if self.scripts {
            SCRIPT_POLICY
        } else if site == SiteKind::Hosted {
            HOSTED_POLICY
        } else {
            POLICY
        };
        let cache = if self.immutable {
            IMMUTABLE
        } else {
            "no-store"
        };
        headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static(policy));
        headers.insert(CACHE_CONTROL, HeaderValue::from_static(cache));
        for (name, value) in FIXED {
            headers.insert(name, HeaderValue::from_static(value));
        }
        if site == SiteKind::Hosted {
            headers.insert(STRICT_TRANSPORT_SECURITY, HeaderValue::from_static(HSTS));
        }
        for (name, value) in extra {
            headers.append(name, value);
        }
        response
    }
}

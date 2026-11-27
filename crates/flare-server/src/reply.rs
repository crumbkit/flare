//! [`Reply`]: every answer the server gives, and the one function that
//! turns it into a response with the threat model's policy and its six
//! other headers, so no handler can leave one out. Every hosted reply adds
//! `connect-src 'self'`. A reading page that loads a script carries the
//! script policy, which also adds `script-src 'self'`, so its comments
//! section can call the comment routes. An embedded asset, whose name holds
//! its content's hash, is cached for a year. A share's image file carries
//! [`FILE_POLICY`], which runs nothing and sandboxes the file when it is
//! opened as a page, is shown inline, and is kept by no shared cache.

use crate::app::bytes_alias::Body;
use crate::site::SiteKind;
use http::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HeaderName,
    HeaderValue, LOCATION, REFERRER_POLICY, SET_COOKIE, STRICT_TRANSPORT_SECURITY,
    X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
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

/// The policy of a share's image file: it loads nothing and runs nothing,
/// and a browser that opens it as a page sandboxes it.
pub(crate) const FILE_POLICY: &str = "default-src 'none'; sandbox";

/// The cache rule of a share's image file: only the reader's browser keeps
/// it, and only while the page shows it.
const PRIVATE: &str = "private, no-store";

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
    file: bool,
    /// The whole policy of a reply from the content host.
    view: Option<String>,
    /// The origin that a reading page may frame, which adds `frame-src`.
    frame_src: Option<String>,
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
            file: false,
            view: None,
            frame_src: None,
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

    /// A share's image file of `content_type`, shown inline under
    /// [`FILE_POLICY`] and kept by no shared cache.
    pub(crate) fn file(status: StatusCode, content_type: &'static str, body: Bytes) -> Self {
        Self {
            file: true,
            ..Self::with_body(status, content_type, body)
        }
    }

    /// A reply of the content host: `body` of `content_type` under the whole
    /// policy `policy`. It carries no cookie and no `X-Frame-Options`, which
    /// would stop the reading page's frame from showing it, and it
    /// sends no referrer.
    pub(crate) fn view(
        status: StatusCode,
        content_type: &'static str,
        body: impl Into<Bytes>,
        policy: String,
    ) -> Self {
        Self {
            view: Some(policy),
            ..Self::with_body(status, content_type, body)
        }
    }

    /// Lets the page frame `origin`: its policy gains `frame-src <origin>`.
    pub(crate) fn with_frame_src(mut self, origin: String) -> Self {
        self.frame_src = Some(origin);
        self
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
        if self.view.is_some() {
            return self.into_view_response();
        }
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
        let policy = if self.file {
            FILE_POLICY
        } else if self.scripts {
            SCRIPT_POLICY
        } else if site == SiteKind::Hosted {
            HOSTED_POLICY
        } else {
            POLICY
        };
        let cache = if self.file {
            PRIVATE
        } else if self.immutable {
            IMMUTABLE
        } else {
            "no-store"
        };
        match self
            .frame_src
            .and_then(|origin| HeaderValue::try_from(format!("{policy}; frame-src {origin}")).ok())
        {
            Some(framing) => headers.insert(CONTENT_SECURITY_POLICY, framing),
            None => headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_static(policy)),
        };
        headers.insert(CACHE_CONTROL, HeaderValue::from_static(cache));
        for (name, value) in FIXED {
            headers.insert(name, HeaderValue::from_static(value));
        }
        if self.file {
            headers.insert(CONTENT_DISPOSITION, HeaderValue::from_static("inline"));
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

const PERMISSIONS_POLICY: HeaderName = HeaderName::from_static("permissions-policy");

/// The content host's `Permissions-Policy`: an empty allowlist for each
/// feature a page could use, so neither the uploaded page nor a frame
/// inside it gets the feature. It names only features that Chromium
/// recognises, since Chromium logs an error for any other name.
const DENY_FEATURES: &str = "accelerometer=(), autoplay=(), camera=(), clipboard-read=(), clipboard-write=(), display-capture=(), encrypted-media=(), fullscreen=(), geolocation=(), gyroscope=(), magnetometer=(), microphone=(), midi=(), payment=(), picture-in-picture=(), publickey-credentials-get=(), screen-wake-lock=(), sync-xhr=(), usb=(), web-share=()";

impl Reply {
    /// The response of the content host, with its policy, no cache, no
    /// referrer, no permission for any browser feature, a separate browsing
    /// context group and `Strict-Transport-Security`. An invalid policy
    /// turns the reply into an empty 500.
    fn into_view_response(self) -> Response<Body> {
        let policy = self
            .view
            .as_deref()
            .and_then(|policy| HeaderValue::try_from(policy).ok());
        let Some(policy) = policy else {
            return Self::empty(StatusCode::INTERNAL_SERVER_ERROR).into_response(SiteKind::Hosted);
        };
        let mut response = Response::new(Body::new(self.body));
        *response.status_mut() = self.status;
        let headers = response.headers_mut();
        if let Some(content_type) = self.content_type {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
        }
        headers.insert(CONTENT_SECURITY_POLICY, policy);
        headers.insert(CACHE_CONTROL, HeaderValue::from_static(PRIVATE));
        headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
        headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
        headers.insert(PERMISSIONS_POLICY, HeaderValue::from_static(DENY_FEATURES));
        headers.insert(COOP, HeaderValue::from_static("same-origin"));
        headers.insert(STRICT_TRANSPORT_SECURITY, HeaderValue::from_static(HSTS));
        response
    }
}

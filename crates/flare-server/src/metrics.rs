//! Counted events as CloudWatch Embedded Metric Format (EMF) lines on the
//! server's log.
//!
//! [`Metrics`] is the one seam: a handler names an [`Event`], and when
//! metrics are on, `record` writes one JSON line through `tracing` with
//! the target [`METRICS_TARGET`]. The `flare` binary's log writes such an
//! event as the bare JSON object, which CloudWatch reads from a Lambda
//! function's log group. Nothing is held in memory and nothing is written
//! to a store, so a cold start or a second instance loses no count.
//!
//! Every value in a line comes from a closed enum or the route table: the
//! route pattern, the status class and the refusal reason. No line holds a
//! person, an address, a token, a session or a share. `docs/metrics.md`
//! lists every metric, and a unit test checks that list against
//! [`Metric::ALL`] and [`Reason::ALL`].
//!
//! A line that the log cannot write is lost, and the request answers as
//! it would have: tracing's writer drops a failed write.

use flare_auth_oidc::SignInRefusal;
use serde_json::{Map, Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

pub use read::Totals;

mod read;
#[cfg(test)]
mod tests;

/// The `tracing` target of every metric line. A log formatter that sees
/// it writes the event's message, which is the whole JSON line, alone.
pub const METRICS_TARGET: &str = "flare::metrics";

/// The CloudWatch namespace of every metric.
pub(crate) const NAMESPACE: &str = "Flare";

/// Whether the server writes metric lines. The default is off.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Metrics {
    on: bool,
}

impl Metrics {
    /// Metrics that write a line for each event when `on` is true, and
    /// write nothing when it is false.
    #[must_use]
    pub const fn new(on: bool) -> Self {
        Self { on }
    }

    /// Writes the line of `event`, which happened at `at`, when metrics are
    /// on.
    pub(crate) fn record(self, event: Event, at: SystemTime) {
        if !self.on {
            return;
        }
        let millis = at.duration_since(UNIX_EPOCH).map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        });
        tracing::info!(target: METRICS_TARGET, "{}", line(event, millis));
    }
}

/// One counted event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Event {
    /// The server answered a request. `route` is the route pattern, `oidc`
    /// for the local sign-in host, or `-` when no route answered.
    Request {
        route: &'static str,
        status: StatusClass,
    },
    /// `POST /api/shares` created a share.
    ShareCreated,
    /// `GET /pages/<name>` answered 200.
    PageViewed,
    /// An OpenID Connect callback signed a browser in.
    SignedIn,
    /// An OpenID Connect callback refused a sign-in.
    SignInRefused { reason: Reason },
    /// The exchange traded a login code for a token.
    TokenExchanged,
    /// The exchange answered 4xx, or another `/api/` request answered 401.
    TokenRefused,
    /// An upload that was refused could not delete a file it had sent.
    FileCleanupFailed,
}

impl Event {
    /// The metric the event counts.
    pub(crate) fn metric(self) -> Metric {
        match self {
            Self::Request { .. } => Metric::Requests,
            Self::ShareCreated => Metric::SharesCreated,
            Self::PageViewed => Metric::PagesViewed,
            Self::SignedIn => Metric::SignIns,
            Self::SignInRefused { .. } => Metric::SignInsRefused,
            Self::TokenExchanged => Metric::TokensExchanged,
            Self::TokenRefused => Metric::TokensRefused,
            Self::FileCleanupFailed => Metric::FilesCleanupFailed,
        }
    }

    /// The values of the metric's dimensions, in the order of
    /// [`Metric::dimensions`].
    fn values(self) -> Vec<&'static str> {
        match self {
            Self::Request { route, status } => vec![route, status.text()],
            Self::SignInRefused { reason } => vec![reason.text()],
            _ => Vec::new(),
        }
    }
}

/// The metrics a flare server writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Metric {
    Requests,
    SharesCreated,
    PagesViewed,
    SignIns,
    SignInsRefused,
    TokensExchanged,
    TokensRefused,
    FilesCleanupFailed,
}

impl Metric {
    /// Every metric, in the order docs/metrics.md lists them.
    #[cfg(test)]
    pub(crate) const ALL: [Self; 8] = [
        Self::Requests,
        Self::SharesCreated,
        Self::PagesViewed,
        Self::SignIns,
        Self::SignInsRefused,
        Self::TokensExchanged,
        Self::TokensRefused,
        Self::FilesCleanupFailed,
    ];

    /// The metric's name in CloudWatch.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Requests => "Requests",
            Self::SharesCreated => "SharesCreated",
            Self::PagesViewed => "PagesViewed",
            Self::SignIns => "SignIns",
            Self::SignInsRefused => "SignInsRefused",
            Self::TokensExchanged => "TokensExchanged",
            Self::TokensRefused => "TokensRefused",
            Self::FilesCleanupFailed => "FilesCleanupFailed",
        }
    }

    /// The names of the metric's dimensions.
    pub(crate) fn dimensions(self) -> &'static [&'static str] {
        match self {
            Self::Requests => &["Route", "StatusClass"],
            Self::SignInsRefused => &["Reason"],
            _ => &[],
        }
    }
}

/// The class of an answer's status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusClass {
    Success,
    Redirect,
    ClientError,
    ServerError,
}

impl StatusClass {
    /// The class of `status`. The server never answers 1xx, so anything
    /// below 300 is a success.
    pub(crate) fn of(status: u16) -> Self {
        match status {
            ..300 => Self::Success,
            300..400 => Self::Redirect,
            400..500 => Self::ClientError,
            _ => Self::ServerError,
        }
    }

    /// The class as CloudWatch shows it, such as `2xx`.
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::Success => "2xx",
            Self::Redirect => "3xx",
            Self::ClientError => "4xx",
            Self::ServerError => "5xx",
        }
    }
}

/// Why a sign-in was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    Malformed,
    NoKeptState,
    WrongState,
    UnknownState,
    ProviderDenied,
    CodeRejected,
    ProviderUnavailable,
    ProviderMisbehaved,
    TokenRejected,
    EmailUnverified,
    NotAllowed,
    TooManySessions,
    NoRandomness,
    CannotSeal,
    NoHostedDomain,
    OtherHostedDomain,
    /// The local server's table of hand-over codes was full.
    NoHandoverSlot,
    /// A refusal that a newer flare-auth-oidc added.
    Other,
}

impl Reason {
    /// Every reason, in the order docs/metrics.md lists them.
    #[cfg(test)]
    pub(crate) const ALL: [Self; 18] = [
        Self::Malformed,
        Self::NoKeptState,
        Self::WrongState,
        Self::UnknownState,
        Self::ProviderDenied,
        Self::CodeRejected,
        Self::ProviderUnavailable,
        Self::ProviderMisbehaved,
        Self::TokenRejected,
        Self::EmailUnverified,
        Self::NotAllowed,
        Self::TooManySessions,
        Self::NoRandomness,
        Self::CannotSeal,
        Self::NoHostedDomain,
        Self::OtherHostedDomain,
        Self::NoHandoverSlot,
        Self::Other,
    ];

    /// The reason of flare-auth-oidc's `refusal`.
    pub(crate) fn of(refusal: SignInRefusal) -> Self {
        match refusal {
            SignInRefusal::Malformed => Self::Malformed,
            SignInRefusal::NoKeptState => Self::NoKeptState,
            SignInRefusal::WrongState => Self::WrongState,
            SignInRefusal::UnknownState => Self::UnknownState,
            SignInRefusal::ProviderDenied => Self::ProviderDenied,
            SignInRefusal::CodeRejected => Self::CodeRejected,
            SignInRefusal::ProviderUnavailable => Self::ProviderUnavailable,
            SignInRefusal::ProviderMisbehaved => Self::ProviderMisbehaved,
            SignInRefusal::TokenRejected(_) => Self::TokenRejected,
            SignInRefusal::EmailUnverified => Self::EmailUnverified,
            SignInRefusal::NotAllowed => Self::NotAllowed,
            SignInRefusal::TooManySessions => Self::TooManySessions,
            SignInRefusal::NoRandomness => Self::NoRandomness,
            SignInRefusal::CannotSeal => Self::CannotSeal,
            SignInRefusal::NoHostedDomain => Self::NoHostedDomain,
            SignInRefusal::OtherHostedDomain => Self::OtherHostedDomain,
            // SignInRefusal is non-exhaustive, so a refusal that a later
            // flare-auth-oidc adds maps to `other` here without a compile
            // error. Name it above, and add it to Reason::ALL, to
            // docs/metrics.md and to the list in
            // every_refusal_of_flare_auth_oidc_has_its_own_reason.
            _ => Self::Other,
        }
    }

    /// The reason as the `Reason` dimension holds it.
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::NoKeptState => "no_kept_state",
            Self::WrongState => "wrong_state",
            Self::UnknownState => "unknown_state",
            Self::ProviderDenied => "provider_denied",
            Self::CodeRejected => "code_rejected",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::ProviderMisbehaved => "provider_misbehaved",
            Self::TokenRejected => "token_rejected",
            Self::EmailUnverified => "email_unverified",
            Self::NotAllowed => "not_allowed",
            Self::TooManySessions => "too_many_sessions",
            Self::NoRandomness => "no_randomness",
            Self::CannotSeal => "cannot_seal",
            Self::NoHostedDomain => "no_hosted_domain",
            Self::OtherHostedDomain => "other_hosted_domain",
            Self::NoHandoverSlot => "no_handover_slot",
            Self::Other => "other",
        }
    }
}

/// The EMF line of `event` at `millis` after the Unix epoch: one JSON
/// object with no line feed.
pub(crate) fn line(event: Event, millis: u64) -> String {
    let metric = event.metric();
    let keys = metric.dimensions();
    let mut root = Map::new();
    root.insert(
        "_aws".to_owned(),
        json!({
            "Timestamp": millis,
            "CloudWatchMetrics": [{
                "Namespace": NAMESPACE,
                "Dimensions": [keys],
                "Metrics": [{ "Name": metric.name(), "Unit": "Count" }],
            }],
        }),
    );
    for (key, value) in keys.iter().zip(event.values()) {
        root.insert((*key).to_owned(), Value::from(value));
    }
    root.insert(metric.name().to_owned(), Value::from(1));
    Value::Object(root).to_string()
}

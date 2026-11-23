//! [`App`]: the stores, the site, the limits and the in-memory tables that
//! every request shares.

use crate::clock::{Clock, SystemClock};
use crate::forms::Forms;
use crate::handlers;
use crate::handover::Handover;
use crate::limits::Limits;
use crate::metrics::{Event, Metrics, StatusClass};
use crate::problem::{Code, Problem};
use crate::schemas::Schemas;
use crate::sharing::{Api, Sharing};
use crate::site::{Site, SiteKind};
use crate::tasks;
use bytes_alias::Body;
use flare_auth_loopback::StateError;
use flare_auth_oidc::{Clock as OidcClock, SystemClock as OidcSystemClock};
use flare_core::{GrantStore, Recipients, ResponseStore, ShareStore, Teams, TokenStore};
use flare_core::{PageStore, RecordStore};
use hyper::body::Incoming;
use hyper::{Request, Response};
use std::fmt;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};
use tokio::sync::{Semaphore, SemaphorePermit};

/// The reply body type every route answers with.
pub(crate) mod bytes_alias {
    /// A whole reply body in memory.
    pub(crate) type Body = http_body_util::Full<hyper::body::Bytes>;
}

/// Which listener a request arrived on.
#[derive(Debug, Clone)]
pub(crate) enum Kind {
    /// The launch host.
    Launch,
    /// The sign-in host, `localhost:<port>`.
    Oidc {
        /// The port its Host header must name.
        port: u16,
    },
    /// The hosted site's one listener.
    Hosted,
}

/// The path of the OpenID Connect callback, which waits for a slot of
/// [`Limits::token_calls`] before it takes its in-work slot.
const CALLBACK: &str = "/auth/oidc/callback";

/// Everything a request needs. Clones share it.
pub struct App<P, R, S, K: OidcClock = OidcSystemClock> {
    shared: Arc<Shared<P, R, S, K>>,
}

impl<P, R, S, K: OidcClock> Clone for App<P, R, S, K> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

/// The state behind an [`App`].
pub(crate) struct Shared<P, R, S, K: OidcClock> {
    pub(crate) pages: P,
    pub(crate) records: R,
    pub(crate) schemas: S,
    pub(crate) site: Site<K>,
    pub(crate) limits: Limits,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) forms: Forms,
    pub(crate) handover: Handover,
    pub(crate) sharing: Option<Arc<dyn Api>>,
    pub(crate) metrics: Metrics,
    pub(crate) in_work: Semaphore,
    token_calls: Semaphore,
    /// Callbacks that hold or wait for a slot of `token_calls`: at most
    /// [`Limits::token_calls`] plus [`Limits::token_waiters`].
    token_queue: Semaphore,
    oidc_origin: OnceLock<String>,
    last_request: Mutex<Instant>,
    next_request: AtomicU64,
}

impl<P, R, S, K> App<P, R, S, K>
where
    P: PageStore + 'static,
    R: RecordStore + 'static,
    S: Schemas + 'static,
    K: OidcClock,
{
    /// An app over the page store `pages`, the record store `records` and
    /// the schemas `schemas`, serving `site` and keeping to `limits`.
    #[must_use]
    pub fn new(pages: P, records: R, schemas: S, site: Site<K>, limits: Limits) -> Self {
        let oidc_origin = OnceLock::new();
        if let Some(origin) = site.origin() {
            let _ = oidc_origin.set(origin.as_str().to_owned());
        }
        let shared = Shared {
            forms: Forms::new(limits.form_tokens, limits.form_token_life),
            handover: Handover::new(limits.handover_codes, limits.handover_life),
            in_work: Semaphore::new(limits.in_work),
            token_calls: Semaphore::new(limits.token_calls),
            token_queue: Semaphore::new(
                limits
                    .token_calls
                    .saturating_add(limits.token_waiters)
                    .min(Semaphore::MAX_PERMITS),
            ),
            sharing: None,
            metrics: Metrics::default(),
            pages,
            records,
            schemas,
            site,
            limits,
            clock: Arc::new(SystemClock),
            oidc_origin,
            last_request: Mutex::new(Instant::now()),
            next_request: AtomicU64::new(1),
        };
        Self {
            shared: Arc::new(shared),
        }
    }

    /// The same app reading time from `clock`. Call it before cloning the
    /// app; on a cloned app it changes nothing.
    #[must_use]
    pub fn with_clock(mut self, clock: impl Clock) -> Self {
        if let Some(shared) = Arc::get_mut(&mut self.shared) {
            shared.clock = Arc::new(clock);
        }
        self
    }

    /// The same app serving the share API from `sharing` on the hosted
    /// site. Call it before cloning the app; on a cloned app it changes
    /// nothing.
    #[must_use]
    pub fn with_sharing<SS, G, RS, T, X, M>(mut self, sharing: Sharing<SS, G, RS, T, X, M>) -> Self
    where
        SS: ShareStore + 'static,
        G: GrantStore + 'static,
        RS: ResponseStore + 'static,
        T: TokenStore + 'static,
        X: Recipients + 'static,
        M: Teams + 'static,
    {
        if let Some(shared) = Arc::get_mut(&mut self.shared) {
            shared.sharing = Some(Arc::new(sharing));
        }
        self
    }

    /// The same app writing a metric line for each counted event when
    /// `metrics` is on. Call it before cloning the app; on a cloned app it
    /// changes nothing.
    #[must_use]
    pub fn with_metrics(mut self, metrics: Metrics) -> Self {
        if let Some(shared) = Arc::get_mut(&mut self.shared) {
            shared.metrics = metrics;
        }
        self
    }

    pub(crate) fn limits(&self) -> &Limits {
        &self.shared.limits
    }

    /// Which site the app serves.
    pub(crate) fn site(&self) -> SiteKind {
        self.shared.site.kind()
    }

    /// Prepares the state directory and writes the server file, on the
    /// local site. The hosted site writes no file.
    pub(crate) fn open_files(&self) -> Result<(), StateError> {
        let shared = &self.shared;
        let Site::Local { gate, state } = &shared.site else {
            return Ok(());
        };
        state.prepare(shared.clock.wall())?;
        state.write_server(&gate.loopback().server_record())
    }

    /// Removes the server file, when it is still this server's, and every
    /// opener this server wrote, on the local site.
    pub(crate) fn close_files(&self) {
        let shared = &self.shared;
        let Site::Local { gate, state } = &shared.site else {
            return;
        };
        if let Err(error) = state.remove_server(&gate.loopback().server_record()) {
            tracing::warn!(error = %error, "the server file could not be removed");
        }
        let far = shared.clock.now() + Duration::from_secs(365 * 24 * 60 * 60);
        for opener in gate.loopback().expire(far) {
            let _ = state.remove_opener(&opener);
        }
    }

    /// Drops expired launch codes, form tokens and hand-over codes every
    /// [`Limits::sweep_every`], removing their openers and signing out the
    /// sessions of hand-over codes that were never redeemed.
    pub(crate) async fn sweep_forever(self) {
        loop {
            tasks::tick(self.shared.limits.sweep_every).await;
            self.sweep();
        }
    }

    pub(crate) fn sweep(&self) {
        let shared = &self.shared;
        let now = shared.clock.now();
        if let Site::Local { gate, state } = &shared.site {
            for opener in gate.loopback().expire(now) {
                let _ = state.remove_opener(&opener);
            }
        }
        shared.forms.expire(now);
        for session in shared.handover.expire(now) {
            if let Some(oidc) = shared.site.oidc() {
                oidc.sign_out(&session);
            }
        }
    }

    /// When the idle period ends unless a request arrives, on the local
    /// site; `None` on the hosted site, which never exits by itself.
    pub(crate) fn idle_deadline(&self) -> Option<Instant> {
        if self.site() == SiteKind::Hosted {
            return None;
        }
        let last = *self
            .shared
            .last_request
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Some(last + self.shared.limits.idle_exit)
    }

    pub(crate) fn idle_passed(&self) -> bool {
        self.idle_deadline()
            .is_some_and(|deadline| Instant::now() >= deadline)
    }

    pub(crate) fn touch(&self) -> u64 {
        *self
            .shared
            .last_request
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Instant::now();
        self.shared.next_request.fetch_add(1, Ordering::Relaxed)
    }

    /// Answers one request.
    pub(crate) async fn respond(
        &self,
        kind: Kind,
        peer: IpAddr,
        request: Request<Incoming>,
    ) -> Response<Body> {
        let id = self.touch();
        let started = Instant::now();
        let method = request.method().clone();
        let headers = request.headers().clone();
        let shared = &self.shared;
        // A callback holds a token request for up to its deadline, so it
        // takes a slot of the callback budget before an in-work slot, and
        // anonymous callbacks cannot take every reader's slot.
        let budget = if request.uri().path() == CALLBACK {
            shared.callback_slot().await.map(Some)
        } else {
            Some(None)
        };
        let held = budget.and_then(|budget| Some((budget, shared.in_work.try_acquire().ok()?)));
        let (reply, route) = match held {
            None => (Problem::from(Code::ServerBusy).reply(&headers), "-"),
            Some(_held) => {
                let work = tasks::caught(handlers::handle(shared, kind, peer, request));
                match tasks::within(shared.limits.request_time, work).await {
                    Some(Ok(answer)) => answer,
                    Some(Err(tasks::Panicked)) => {
                        tracing::error!(request = id, "a handler panicked; answered 500");
                        (Problem::from(Code::Fault).reply(&headers), "-")
                    }
                    None => (Problem::from(Code::Deadline).reply(&headers), "-"),
                }
            }
        };
        let millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        tracing::info!(
            request = id,
            method = %method,
            route,
            status = reply.status().as_u16(),
            ms = millis,
            "served"
        );
        shared.count(Event::Request {
            route,
            status: StatusClass::of(reply.status().as_u16()),
        });
        reply.into_response(self.site())
    }

    /// Records that the sign-in host listens on `port`, so the 401 document
    /// links to its start route.
    pub(crate) fn serve_sign_in_on(&self, port: u16) {
        let _ = self
            .shared
            .oidc_origin
            .set(format!("http://localhost:{port}"));
    }
}

impl<P, R, S, K: OidcClock> Shared<P, R, S, K> {
    /// Writes the metric line of `event`, when metrics are on.
    pub(crate) fn count(&self, event: Event) {
        self.metrics.record(event, self.clock.wall());
    }

    /// The sign-in host's origin, when it listens.
    pub(crate) fn oidc_origin(&self) -> Option<&str> {
        self.oidc_origin.get().map(String::as_str)
    }

    /// A slot of the callback budget, with the callback's place in the
    /// queue. It is `None` at once when [`Limits::token_waiters`] callbacks
    /// already wait for a slot, and `None` when no slot frees within
    /// [`Limits::token_wait`]. Tokio's semaphore hands slots out in the
    /// order callbacks asked for them, so a callback waits only behind the
    /// callbacks that arrived before it.
    async fn callback_slot(&self) -> Option<CallbackSlot<'_>> {
        let queued = self.token_queue.try_acquire().ok()?;
        let slot = tasks::within(self.limits.token_wait, self.token_calls.acquire())
            .await?
            .ok()?;
        Some((queued, slot))
    }
}

/// A callback's place in the budget's queue and its slot, released together
/// when the callback answers.
type CallbackSlot<'a> = (SemaphorePermit<'a>, SemaphorePermit<'a>);

impl<P, R, S, K: OidcClock> fmt::Debug for App<P, R, S, K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("App")
            .field("site", &self.shared.site)
            .field("limits", &self.shared.limits)
            .finish_non_exhaustive()
    }
}

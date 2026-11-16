//! [`App`]: the stores, the authenticators, the state directory, the
//! limits and the in-memory tables that every request shares.

use crate::auth::Gate;
use crate::clock::{Clock, SystemClock};
use crate::forms::Forms;
use crate::handlers;
use crate::handover::Handover;
use crate::limits::Limits;
use crate::problem::{Code, Problem};
use crate::schemas::Schemas;
use crate::tasks;
use bytes_alias::Body;
use hangar_auth_loopback::{StateDir, StateError};
use hangar_auth_oidc::{Clock as OidcClock, SystemClock as OidcSystemClock};
use hangar_core::{PageStore, RecordStore};
use hyper::body::Incoming;
use hyper::{Request, Response};
use std::fmt;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

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
}

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
    pub(crate) gate: Gate<K>,
    pub(crate) state: StateDir,
    pub(crate) limits: Limits,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) forms: Forms,
    pub(crate) handover: Handover,
    pub(crate) in_work: Semaphore,
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
    /// the schemas `schemas`, signing people in through `gate`, keeping its
    /// server file and openers in `state`, and keeping to `limits`.
    #[must_use]
    pub fn new(
        pages: P,
        records: R,
        schemas: S,
        gate: Gate<K>,
        state: StateDir,
        limits: Limits,
    ) -> Self {
        let shared = Shared {
            forms: Forms::new(limits.form_tokens, limits.form_token_life),
            handover: Handover::new(limits.handover_codes, limits.handover_life),
            in_work: Semaphore::new(limits.in_work),
            pages,
            records,
            schemas,
            gate,
            state,
            limits,
            clock: Arc::new(SystemClock),
            oidc_origin: OnceLock::new(),
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

    pub(crate) fn limits(&self) -> &Limits {
        &self.shared.limits
    }

    /// Prepares the state directory and writes the server file.
    pub(crate) fn open_files(&self) -> Result<(), StateError> {
        let shared = &self.shared;
        shared.state.prepare(shared.clock.wall())?;
        shared
            .state
            .write_server(&shared.gate.loopback().server_record())
    }

    /// Removes the server file, when it is still this server's, and every
    /// opener this server wrote.
    pub(crate) fn close_files(&self) {
        let shared = &self.shared;
        if let Err(error) = shared
            .state
            .remove_server(&shared.gate.loopback().server_record())
        {
            tracing::warn!(error = %error, "the server file could not be removed");
        }
        let far = shared.clock.now() + Duration::from_secs(365 * 24 * 60 * 60);
        for opener in shared.gate.loopback().expire(far) {
            let _ = shared.state.remove_opener(&opener);
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
        for opener in shared.gate.loopback().expire(now) {
            let _ = shared.state.remove_opener(&opener);
        }
        shared.forms.expire(now);
        for session in shared.handover.expire(now) {
            if let Some(oidc) = shared.gate.oidc() {
                oidc.sign_out(&session);
            }
        }
    }

    /// When the idle period ends unless a request arrives.
    pub(crate) fn idle_deadline(&self) -> Instant {
        let last = *self
            .shared
            .last_request
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        last + self.shared.limits.idle_exit
    }

    pub(crate) fn idle_passed(&self) -> bool {
        Instant::now() >= self.idle_deadline()
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
        let (reply, route) = match shared.in_work.try_acquire() {
            Err(_) => (Problem::from(Code::ServerBusy).reply(&headers), "-"),
            Ok(_permit) => {
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
        reply.into_response()
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
    /// The sign-in host's origin, when it listens.
    pub(crate) fn oidc_origin(&self) -> Option<&str> {
        self.oidc_origin.get().map(String::as_str)
    }
}

impl<P, R, S, K: OidcClock> fmt::Debug for App<P, R, S, K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("App")
            .field("gate", &self.shared.gate)
            .field("limits", &self.shared.limits)
            .finish_non_exhaustive()
    }
}

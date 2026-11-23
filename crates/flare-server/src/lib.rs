//! flare's HTTP server: it serves pages and bindings from a board, draws
//! and checks edit forms, and signs people in, either on a person's own
//! machine or behind a TLS-terminating proxy.
//!
//! The server reads no environment variable and starts no process. Its
//! caller, the `flare` binary, builds the stores and a [`Site`] and hands
//! them to [`App::new`], binds with [`Listeners::bind`] or
//! [`Listeners::bind_hosted`], and calls [`Listeners::serve`]. [`ops`]
//! holds the two writes that need no server: publishing a page and saving
//! a binding.
//!
//! # Sites and listeners
//!
//! [`Site::Local`] is the server on a person's own machine:
//!
//! - The launch host, `h-<32 hex>.localhost:<port>` on `127.0.0.1`, serves
//!   every route of the table in `routes.rs` that names the local site.
//! - With OpenID Connect, `localhost:<oidc port>` on `127.0.0.1` and `::1`
//!   serves only `/auth/oidc/start` and `/auth/oidc/callback`, and never
//!   sets the session cookie.
//!
//! [`Site::Hosted`] is one plain-HTTP listener behind a TLS proxy, serving
//! one [`PublicOrigin`]. It runs `hosted.rs`'s checks (the Host, the Origin
//! of a non-GET request, the exact `__Host-flare_session` cookie and no
//! Authorization header), then the same cross-site rule, and serves only
//! the routes that name the hosted site: `/up`, the stylesheet, `/`,
//! `/flare/`, `/pages/<name>`, `/auth/oidc/start` and
//! `/auth/oidc/callback`. It writes no board record. It sets only
//! `__Host-` cookies, writes no file and has no idle exit.
//!
//! # The share API
//!
//! [`App::with_sharing`] adds the share API to the hosted site:
//! `/cli/login`, which hands a signed-in browser's one-use login code to
//! the `flare` command's loopback listener, `POST /api/tokens/exchange`,
//! which trades the code for a bearer token, and the token routes under
//! `/api/` that share a file, grant it, list and open shares, read the
//! responses to flares since a cursor and revoke a token. For an `/api/`
//! path the hosted checks read no cookie and require no Origin; the
//! credential is the one `Authorization: Bearer` header. Without
//! [`Sharing`] these routes answer 404. The lifecycle of login codes and
//! tokens is in `docs/tokens.md`.
//!
//! # Metrics
//!
//! [`App::with_metrics`] with [`Metrics::new`]`(true)` writes one
//! CloudWatch Embedded Metric Format line for each request, created share,
//! page view, sign-in, refused sign-in, token exchange and refused token,
//! through `tracing` with the target [`METRICS_TARGET`]. Metrics are off by
//! default. No route serves the counts, and no line names a person, a
//! token, a session or a share. [`Totals`] counts such lines in a log.
//! `docs/metrics.md` lists every metric and dimension.
//!
//! Every reply, refusals included, carries the threat model's
//! Content-Security-Policy and six other headers (`reply.rs`), and every
//! hosted reply also `Strict-Transport-Security`.
//!
//! # Lifecycle
//!
//! This is the landed copy of the lifecycle records. The server file, the
//! openers, launch codes and loopback sessions follow flare-auth-loopback's
//! table, and pending sign-ins and OpenID Connect sessions follow
//! flare-auth-oidc's. The hosted site never creates the server file, the
//! openers, launch codes, loopback sessions or hand-over codes.
//!
//! The local server process (`flare serve` without a `hosted` section):
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | checking | the command starting | the board program, board directory, config, Keychain read and provider discovery passing; or a coded failure and exit |
//! | bound | [`Listeners::bind`] | `StateDir::prepare` and `write_server` passing; or a failure, which closes the listeners and exits |
//! | serving | `write_server` | Ctrl-C or SIGTERM; [`Limits::idle_exit`] without a request; a listener error |
//! | draining | any of the three | every request in work finishing; or [`Limits::drain`] passing, which drops the requests still in work and waits until each board child they started has ended |
//! | stopped | draining ending, then `remove_server` for its own record and removal of its openers | final |
//! | killed | SIGKILL, a power loss, a crash; `server.json` is stale | the next `flare serve`, which replaces the file |
//!
//! The hosted server process (`flare serve` with a `hosted` section):
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | checking | the command starting | the board program, board directory, config, secret file, sealing key and provider discovery passing; or a coded failure and exit, with nothing bound |
//! | bound | [`Listeners::bind_hosted`] on the configured address | serving starting; or a failure, which closes the listener and exits |
//! | serving | the accept loop starting; no server file is written | Ctrl-C or SIGTERM; a listener error. There is no idle exit |
//! | draining | either of the above | every request in work finishing; or [`Limits::drain`] passing, which drops the requests still in work and waits until each board child they started has ended |
//! | stopped | draining ending | final; every session and the sealing key are gone with the process |
//! | killed | SIGKILL, a crash, a host restart | final; nothing on disk to repair, and the next start serves with an empty session table and a new key |
//!
//! A hosted session (one browser):
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | signed in | `complete` succeeding, which removes the session of the browser's current cookie, if any, in the same step that creates the new one; the callback sets `__Host-flare_session` (Lax, Max-Age = session life) and answers 303 to the return path | its expiry; a restart; a newer successful sign-in in the same browser; the same principal's 9th sign-in anywhere, which ends its earliest-expiring session. Removal from the admit list does not end it |
//! | refused | a kept value that does not open, is expired or has another state; a provider refusal; a full session table | final; no cookie set; any older session of the browser stays live |
//!
//! Before it binds, `flare serve` reads `server.json` and sends the probe;
//! a right proof fails the command with `server::already_running`. A timer
//! calls `Loopback::expire` every [`Limits::sweep_every`], removes the
//! openers it returns, and drops expired form tokens and hand-over codes.
//! A dropped call to flare-board, from the request deadline or the drain,
//! stops the board child's process group and blocks until the child has
//! ended. A write the child had committed before the stop stays committed;
//! any other write never commits.
//!
//! A form token (memory, at most [`Limits::form_tokens`], and at most an
//! eighth of them for one session):
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | issued | a form or conflict view being drawn for a session, binding, slot, record path and blob | a post that carries it; [`Limits::form_token_life`] passing; its session drawing a token while it holds its share, which drops that session's oldest row; the table filling, which drops the oldest row |
//! | spent | the post's handler removing the row, before any comparison | final; a second post with it reads as unknown |
//! | expired or dropped | the time passing, the session's share or the table filling, or the server exiting | final; reads as unknown |
//!
//! An edit (one POST to `/b/<name>`):
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | drawn | GET `/b/<name>` drawing a form with a new token | a post |
//! | posted | the request checks, the body limit and `FormPost::from_pairs` passing | the token comparison |
//! | refused | an unknown or mismatched token, a changed binding, a denied `authorize`, a `PostError`, an `ApplyError` or a rejected write | final for the token; the form is drawn again with a new token where the session is still trusted |
//! | unchanged | `Outcome::Unchanged` | 303 to the binding |
//! | writing | `Outcome::Write` handed to `RecordStore::write` | written; `WriteError::Conflict`, which reads the record and applies again, at most three times; `Unavailable`; [`Limits::request_time`] passing, which drops the write, waits for its board child to end and then answers 503 `server::deadline` |
//! | written | the store answering with the new blob | 303 to the binding |
//! | in conflict | `Outcome::Conflict` | the conflict view, drawn with a new token tied to the newer blob; its post is a new edit |
//!
//! A hand-over code (memory, at most [`Limits::handover_codes`]):
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | issued | `/auth/oidc/callback` after `complete` | `/auth/oidc/finish` within [`Limits::handover_life`]; the time passing |
//! | redeemed | the finish route removing the row and setting the session cookie | final |
//! | expired | the time passing, the table filling or the server exiting | final; the OpenID Connect session it held is signed out |
//!
//! The kept-state cookie, `flare_oidc_state` on the local sign-in host and
//! `__Host-flare_oidc_state` on the hosted site, holds the sealed pending
//! sign-in:
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | set | `/auth/oidc/start` | `/auth/oidc/callback`, which clears it on every answer; 600 seconds passing |
//! | cleared | either of the above | final |
//!
//! A callback's place in the callback budget, on both sites (memory, at most
//! [`Limits::token_calls`] holding and [`Limits::token_waiters`] waiting):
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | waiting | `/auth/oidc/callback` arriving while every slot is taken and fewer than `token_waiters` callbacks wait | a slot freeing, which goes to the callback that has waited longest; [`Limits::token_wait`] passing |
//! | holding | a free slot on arrival or while waiting, then an in-work slot | the reply, which frees both |
//! | refused | the queue already full, the wait passing, or no in-work slot | final; 503 `server::busy`, with no token request sent |
//!
//! # Crash points
//!
//! 1. Death after bind and before `write_server` leaves nothing behind.
//! 2. Death while serving leaves a stale `server.json` and maybe openers; the
//!    next `flare serve` finds no right proof, sweeps the directory and
//!    writes its own file. `flare open` in between fails with
//!    `server::not_running`.
//! 3. Death between a record write and the 303 leaves the record and its
//!    edit record committed; a reload draws the form from the new blob, and
//!    a resubmitted post carries a token the new server never issued.
//! 4. Death during a record write leaves what flare-board's lifecycle says:
//!    a whole commit or files left uncommitted.
//! 5. Death between the callback and the finish route loses the hand-over
//!    code and the session; the person signs in again.
//! 6. A panic in a request's task is caught by the task owner, answered with
//!    500 `server::fault` and logged with the request ID. A token it spent
//!    stays spent. A poisoned table answers 503 for its routes:
//!    `forms::unavailable` for a post and `launch::unavailable` for the
//!    finish route.
//! 7. Draining that passes [`Limits::drain`] with a write in work aborts the
//!    task, which drops the write. flare-board stops the board child with
//!    its process group and blocks until it has ended, so `serve` returns
//!    only after the child is gone. What the board holds is point 4.
//! 8. A failed `remove_server` at exit leaves a stale file, as in point 2.
//! 9. Two `flare serve` commands that start at once for one board both see
//!    no current server. The later rename of `server.json` wins, and the
//!    other serves only browsers already signed in until its idle exit.
//! 10. On the hosted site, death between `start` and the callback voids the
//!     sealed value with the key; the callback answers `oidc::refused` and
//!     clears the kept cookie. Death between `complete` and the reply
//!     leaves the browser with its old cookie, which no longer
//!     authenticates, and the person signs in again. `complete` removes the
//!     old session and creates the new one under one lock.
//!
//! # Invariants
//!
//! 1. `server.json` is written only after every listener is bound, and a
//!    server removes only its own record.
//! 2. No GET route writes a record, a page or a binding.
//! 3. A record is written only by a post whose token this process issued to
//!    the same session, binding and slot, for the blob the post names, and
//!    whose record path comes from the binding.
//! 4. A token is removed from its table before the post's other checks run,
//!    so no token is accepted twice, even by two posts at once.
//! 5. On the local site, the session cookie is set only on the launch host,
//!    by `/launch` and `/auth/oidc/finish`. On the hosted site,
//!    `__Host-flare_session` is set only by `/auth/oidc/callback`, only
//!    after `complete` succeeds with a sealed kept value from the same
//!    browser, and its secret is minted by `complete`, never read from the
//!    request.
//! 6. Every `authorize` call goes to the authenticator that accepted the
//!    request's credential, in the same request. Nothing stores a principal
//!    or a decision between requests.
//! 7. Only `tasks.rs` spawns a task, and every task it spawns ends before
//!    [`Listeners::serve`] returns.
//! 8. No secret reaches the log, a command's output, an error body or a
//!    `Debug` output.
//! 9. Every reply carries the policy and the six other headers.
//! 10. No board child that a request started runs after the server has
//!     answered that request, and none runs after [`Listeners::serve`]
//!     returns.
//! 11. The hosted site serves only requests whose one Host header is its
//!     origin's authority, and reads no forwarded header or peer address.
//! 12. The hosted site sets only `__Host-` cookies, each with `Secure`,
//!     `HttpOnly` and `Path=/`, and clears them with the same attributes.
//! 13. The hosted site serves none of `/b/<name>`, `/ready`, `/launch`,
//!     `/launch/codes`, `/launch/probe` and `/auth/oidc/finish`, writes no
//!     file or board record, and stores nothing server-side for an
//!     anonymous request. Its only routes with another method than GET
//!     are under `/api/`, and each takes a bearer token, or a login code
//!     for the exchange, and never a cookie.
//! 14. At most [`Limits::token_calls`] callbacks are in work at once. The
//!     next waits for a slot, in arrival order, for up to
//!     [`Limits::token_wait`], and at most [`Limits::token_waiters`] wait
//!     at once. A waiting callback holds no in-work slot. A callback that
//!     finds the queue full, or whose wait runs out, answers 503
//!     `server::busy` before it takes an in-work slot, on both sites.
//! 15. A hosted flare runs as one process, because its sessions and
//!     sealing key live in that process's memory. A second process behind
//!     the same origin refuses the first one's kept values and sessions.
//! 16. The server never stores a raw token or login code, and never logs
//!     one: the token store keeps SHA-256 digests, and a token's text
//!     leaves the process only in the exchange's answer.

mod app;
mod auth;
mod checks;
mod clock;
mod cookies;
mod docs;
mod forms;
mod frame;
mod handlers;
mod handover;
mod hosted;
mod launch;
mod limits;
mod listen;
mod metrics;
pub mod ops;
mod problem;
mod reply;
mod routes;
mod schemas;
mod secret;
mod sharing;
mod site;
mod tasks;

/// Every route the local site's launch host serves, as its method and
/// pattern. A pattern names its one variable segment `<name>`.
pub fn routes() -> impl Iterator<Item = (http::Method, &'static str)> {
    routes::listed()
}

pub use app::App;
pub use auth::{Admitted, Gate, Side};
pub use clock::{Clock, SystemClock};
pub use limits::Limits;
pub use listen::{BindError, Listeners, ServeError, Stopped};
pub use metrics::{METRICS_TARGET, Metrics, Totals};
pub use schemas::Schemas;
pub use sharing::Sharing;
pub use site::{OriginError, PublicOrigin, Site};

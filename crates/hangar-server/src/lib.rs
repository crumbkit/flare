//! hangar's HTTP server: it serves pages and bindings from a board, draws
//! and checks edit forms, and signs people in, on loopback addresses only.
//!
//! The server reads no environment variable and starts no process. Its
//! caller, the `hangar` binary, builds the stores, the authenticators and
//! the state directory and hands them to [`App::new`], binds with
//! [`Listeners::bind`], and calls [`Listeners::serve`]. [`ops`] holds the
//! two writes that need no server: publishing a page and saving a binding.
//!
//! # Listeners
//!
//! - The launch host, `h-<32 hex>.localhost:<port>` on `127.0.0.1`, serves
//!   every route of the table in `routes.rs`.
//! - With OpenID Connect, `localhost:<oidc port>` on `127.0.0.1` and `::1`
//!   serves only `/auth/oidc/start` and `/auth/oidc/callback`, and never
//!   sets the session cookie.
//!
//! Every reply, refusals included, carries the threat model's
//! Content-Security-Policy and six other headers (`reply.rs`).
//!
//! # Lifecycle
//!
//! The server process:
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | bound | [`Listeners::bind`] | [`Listeners::serve`] writing `server.json` after `StateDir::prepare`; a failure, which closes the listeners |
//! | serving | `write_server` | the shutdown future; the idle period passing; a listener error |
//! | draining | any of the three | every request in work finishing, or [`Limits::drain`] passing |
//! | stopped | draining ending, then `remove_server` and removal of the openers | final |
//!
//! A form token (memory, at most [`Limits::form_tokens`]):
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | issued | a form or conflict view being drawn for a session, binding, slot, record path and blob | a post that carries it; [`Limits::form_token_life`] passing; the table filling, which drops the oldest row |
//! | spent | the post's handler removing the row, before any comparison | final |
//! | expired or dropped | the time passing, the table filling or the server exiting | final |
//!
//! An edit (one POST to `/b/<name>`):
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | drawn | GET `/b/<name>` drawing a form with a new token | a post |
//! | posted | the request checks, the body limit and `FormPost::from_pairs` passing | the token comparison |
//! | refused | an unknown or mismatched token, a changed binding, a denied `authorize`, a `PostError`, an `ApplyError` or a rejected write | final for the token; the form is drawn again with a new token where the session is still trusted |
//! | unchanged | `Outcome::Unchanged` | 303 to the binding |
//! | writing | `Outcome::Write` handed to `RecordStore::write` | written; `WriteError::Conflict`, which reads the record and applies again, at most three times; `Unavailable` |
//! | written | the store answering with the new blob | 303 to the binding |
//! | in conflict | `Outcome::Conflict` | the conflict view, drawn with a new token tied to the newer blob |
//!
//! A hand-over code (memory, at most [`Limits::handover_codes`]):
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | issued | `/auth/oidc/callback` after `complete` | `/auth/oidc/finish` within [`Limits::handover_life`]; the time passing |
//! | redeemed | the finish route removing the row and setting the session cookie | final |
//! | expired | the time passing, the table filling or the server exiting | final; the OpenID Connect session it held is signed out |
//!
//! The kept-state cookie `hangar_oidc_state` on `localhost` is set by
//! `/auth/oidc/start` and cleared by `/auth/oidc/callback` on every answer,
//! or after 600 seconds.
//!
//! # Crash points
//!
//! 1. Death after bind and before `write_server` leaves nothing behind.
//! 2. Death while serving leaves a stale `server.json` and maybe openers; the
//!    next `hangar serve` finds no right proof, sweeps the directory and
//!    writes its own file.
//! 3. Death between a record write and the 303 leaves the record written; a
//!    resubmitted post carries a token the new server never issued.
//! 4. Death during a record write leaves what hangar-board's lifecycle says.
//! 5. Death between the callback and the finish route loses the hand-over
//!    code and the session; the person signs in again.
//! 6. A panic in a request's task is caught by the task owner, answered with
//!    500 `server::fault` and logged with the request ID. A token it spent
//!    stays spent.
//! 7. Draining that passes [`Limits::drain`] with a write in work aborts the
//!    task, which is point 4.
//! 8. A failed `remove_server` at exit leaves a stale file, as in point 2.
//!
//! # Invariants
//!
//! 1. `server.json` is written only after every listener is bound, and a
//!    server removes only its own record.
//! 2. No GET route writes a record, a page or a binding.
//! 3. A record is written only by a post whose token this process issued to
//!    the same session, binding and slot, for the blob the post names, and
//!    whose record path comes from the binding.
//! 4. A token is removed from its table before the post's other checks run.
//! 5. The session cookie is set only on the launch host, by `/launch` and
//!    `/auth/oidc/finish`.
//! 6. Every `authorize` call goes to the authenticator that accepted the
//!    request's credential, in the same request. Nothing stores a principal
//!    or a decision between requests.
//! 7. Only `tasks.rs` spawns a task, and every task it spawns ends before
//!    [`Listeners::serve`] returns.
//! 8. No secret reaches the log, an error body or a `Debug` output.
//! 9. Every reply carries the policy and the six other headers.

mod app;
mod auth;
mod checks;
mod clock;
mod docs;
mod forms;
mod frame;
mod handlers;
mod handover;
mod launch;
mod limits;
mod listen;
pub mod ops;
mod problem;
mod reply;
mod routes;
mod schemas;
mod secret;
mod tasks;

/// Every route of the launch host's table, as its method and pattern. A
/// pattern names its one variable segment `<name>`.
pub fn routes() -> impl Iterator<Item = (http::Method, &'static str)> {
    routes::listed()
}

pub use app::App;
pub use auth::{Admitted, Gate, Side};
pub use clock::{Clock, SystemClock};
pub use limits::Limits;
pub use listen::{BindError, Listeners, ServeError, Stopped};
pub use schemas::Schemas;

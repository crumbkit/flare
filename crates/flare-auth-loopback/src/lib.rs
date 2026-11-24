//! Signs in the person who runs a local flare server, and implements
//! [`flare_core::Authenticator`] for the token and the sessions it makes.
//!
//! The crate holds no sockets, routes or pages, reads no clock and no
//! environment variable, and never sleeps. The server passes each
//! request's peer, method and headers to [`Loopback::check`], and passes
//! the current [`Instant`](std::time::Instant) to the calls that need it.
//!
//! # Sign-in
//!
//! 1. `flare serve` makes a [`Loopback`], which picks a random launch host
//!    `h-<32 hex>.localhost` and a bearer token, and writes them to the
//!    server file with [`StateDir::write_server`].
//! 2. `flare open` reads the server file, sends a fresh [`Nonce`] to the
//!    probe, and checks the answer with [`ServerRecord::proof_matches`]. It
//!    sends the token only after a right proof.
//! 3. With the token, it asks for a code. The server calls
//!    [`Loopback::issue_code`], writes [`Loopback::opener_page`] with
//!    [`StateDir::write_opener`], ties the two with
//!    [`Loopback::keep_opener`], and answers with the opener's path.
//! 4. The browser follows the opener to `/launch?code=<code>`. The server
//!    calls [`Loopback::sign_in`], removes [`SignedIn::opener`], and sets
//!    [`SignedIn::set_cookie`]. The target is bound to the code, so it
//!    never travels in the URL.
//!
//! # Request checks, in order
//!
//! 1. The peer is a loopback address, or [`Refusal::Peer`].
//! 2. The request has one `Host` header, equal to
//!    `h-<32 hex>.localhost:<port>`, or [`Refusal::Host`].
//! 3. A method other than GET and HEAD needs `Origin` equal to the launch
//!    origin and `Sec-Fetch-Site: same-origin`. A request with an
//!    `Authorization` header and neither of those headers passes, because a
//!    browser never adds one by itself. Otherwise [`Refusal::Origin`].
//! 4. `Authorization: Bearer <token>` gives a bearer credential; without
//!    it, the `flare_session` cookie gives a session credential. A
//!    malformed or repeated `Authorization` header, or two session cookies,
//!    is [`Refusal::Credential`].
//!
//! # Limits
//!
//! - A launch code works once, for 60 seconds. Its digest is kept for 10
//!   minutes after it was made, and the code is then unknown.
//! - At most 64 codes are unused and unexpired at once; a 65th is
//!   [`CodeError::TooMany`] until one is used or expires.
//! - At most 64 sessions are live; a 65th replaces the oldest, whose
//!   browser must sign in again. Sessions end when the server exits.
//!
//! # Launch code
//!
//! | State | Event | Next state |
//! | --- | --- | --- |
//! | absent | `issue_code` with fewer than 64 outstanding | unused |
//! | unused | `sign_in` within 60 seconds | used, and a session is live |
//! | unused | 60 seconds pass | expired; `expire` hands back its opener |
//! | used or expired | 10 minutes after it was made, at `expire` or `issue_code` | forgotten; a kept opener comes back from the next `expire` |
//!
//! `sign_in` marks the code used and stores the session under one lock.
//! It reads the session's random bytes before it takes the lock, so a
//! failure there leaves the code unused.
//!
//! # Session
//!
//! | State | Event | Next state |
//! | --- | --- | --- |
//! | absent | a successful `sign_in` | live |
//! | live | a 65th session, when it is the oldest | ended |
//! | live | the server exits | ended |
//!
//! # Files
//!
//! [`StateDir`] owns `<state home>/flare/loopback/<board key>/`, mode
//! 0700, which holds `server.json` and the openers, each mode 0600. The
//! crate refuses a directory or file that is a link, belongs to another
//! user, has another mode or, for a file, has a second link, and it never
//! changes a mode it finds. It removes only `server.json` when the file
//! holds the caller's token, and temporary files and openers it named.
//!
//! ## Server file (`server.json`)
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | absent | first use; `remove_server` by the server it names; a person removing the directory | `write_server` |
//! | current | `write_server`: crumbkit-durable writes a temporary file, syncs it, renames it and syncs the directory | `remove_server` at exit; `write_server` by a newer server; the server dying |
//! | stale | the server dying without `remove_server` | `write_server` by the next server |
//! | untrusted | something outside flare changing its owner, mode, links or type | `write_server`, which replaces it by rename and never opens it; a person removing it |
//!
//! `read_server` cannot tell current from stale. A client tells them apart
//! with the probe: a current server answers the proof for a fresh nonce,
//! and a stale file's port answers nothing or a wrong proof.
//!
//! ## Opener (`open-<16 hex>.html`)
//!
//! | State | Entered by | Left by |
//! | --- | --- | --- |
//! | written | `write_opener` after `issue_code` | `remove_opener` after `sign_in` hands it back; `expire` handing it back; the sweep in `prepare` once it is over 60 seconds old |
//! | removed | any of those | final |
//!
//! ## Crash points
//!
//! 1. Death after the temporary file is made and before the rename leaves
//!    `server.json` unchanged; a later sweep removes the temporary file.
//! 2. Death after the rename and before the directory sync leaves the new
//!    file; after a power loss it may be the old file or absent.
//! 3. Death while serving leaves a stale file. Codes and sessions die with
//!    the process, and the next server's `write_server` replaces the file.
//! 4. Death between `issue_code` and `write_opener`, or after it, or after
//!    `sign_in` and before the opener is removed, leaves an opener with a
//!    dead code; the sweep removes it.
//! 5. Two servers starting for one board both rename, and the later rename
//!    wins. The other server keeps serving browsers already signed in to it.
//!    No lock prevents this.
//! 6. A server exiting while a newer one replaces the file compares tokens
//!    and then unlinks. A rename between the two removes the newer file;
//!    the next client finds no file and starts another server.
//!
//! The crate keeps launch codes and session secrets only as SHA-256
//! digests, and no `Debug` output of the crate shows a token, code,
//! session secret or launch host name.

mod codes;
mod host;
mod loopback;
mod principal;
mod probe;
mod record;
mod request;
mod secret;
mod state;

pub use codes::{CodeError, LaunchCode, ReturnPath, ReturnPathError, SignInError, SignedIn};
pub use host::LaunchHost;
pub use loopback::{KeepOpenerError, Loopback, StartError};
pub use principal::{Served, local_principal};
pub use probe::{Nonce, NonceError};
pub use record::ServerRecord;
pub use request::{Checked, Refusal};
pub use state::{ReadStep, StateDir, StateError, Untrusted, state_home};

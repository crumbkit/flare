//! Signs people in to hangar with an OpenID Connect provider, such as
//! Google, and implements [`hangar_core::Authenticator`] for the sessions it
//! creates.
//!
//! [`OidcAuthenticator::connect`] reads the client secret, fetches the
//! provider's discovery document and key set, and checks both.
//! [`OidcAuthenticator::start`] returns the provider URL to send the browser
//! to and the state for the server to keep in a cookie.
//! [`OidcAuthenticator::complete`] takes the callback's query and the kept
//! state, exchanges the code with PKCE, verifies the ID token, checks the
//! email against the allow-list and creates a session. The crate holds no
//! HTTP routes, cookies or pages; the server that hosts it does.
//!
//! The crate accepts only ID tokens signed with RS256, asks for no refresh token
//! and stores no ID token or access token. Everything it keeps lives in the
//! memory of one process, so a restart signs everyone out and leaves nothing
//! to repair.
//!
//! # ID token checks, in order
//!
//! 1. The header's `alg` is RS256. Any other value, `none` and HS256
//!    included, is refused before a key is looked up.
//! 2. The header's `kid` names a key in the cached key set. For an unknown
//!    `kid` the key set is fetched again, at most once every 60 seconds. A
//!    sign-in that needs a fetch while another is fetching waits for it.
//! 3. The RS256 signature verifies with that key.
//! 4. `iss` equals the configured issuer (for `https://accounts.google.com`,
//!    `accounts.google.com` too, as Google documents), `aud` is the client ID alone,
//!    `azp` equals the client ID when present, and `exp` is in the future
//!    with 60 seconds of leeway.
//! 5. `nonce` equals the pending sign-in's nonce.
//! 6. `email` is present and `email_verified` is the JSON boolean `true`.
//! 7. The email is ASCII, and lower-cased it is on the allow-list, whose
//!    entry gives the principal.
//!
//! # Limits
//!
//! - At most 256 sign-ins are pending, each for 10 minutes. A `start` at the
//!   limit removes the oldest, whose callback then gets
//!   [`SignInRefusal::UnknownState`].
//! - At most 1,024 sessions are live, each for 12 hours unless
//!   [`OidcConfig::with_session_lifetime`] sets another lifetime. A sign-in
//!   at the limit, after expired sessions are removed, gets
//!   [`SignInRefusal::TooManySessions`].
//! - A callback query of more than 4,096 bytes is
//!   [`SignInRefusal::Malformed`].
//! - A reply from the provider over 64 KiB, or past the 5 second connect
//!   deadline or the 10 second total deadline, is
//!   [`SignInRefusal::ProviderUnavailable`], or
//!   [`ConnectError::Unreachable`] during `connect`. The client follows no
//!   redirects.
//!
//! # Pending sign-in
//!
//! Owned by `flow.rs`, keyed by the SHA-256 of the state. It holds the
//! nonce, the PKCE verifier, the return-to value and the time it was made.
//!
//! | State | Event | Guard | Next state |
//! | --- | --- | --- | --- |
//! | absent | `start` | none | pending |
//! | pending | `complete` with the matching kept state | the query's state equals the kept state, compared in constant time; the entry is under 10 minutes old | used (removed) |
//! | pending | `complete` with a different or missing kept state | none | pending (unchanged) |
//! | pending | 10 minutes pass | checked when the entry is looked up and when `start` makes room | expired (removed) |
//! | pending | `start` while 256 are pending | it is the oldest | evicted (removed) |
//!
//! The entry is removed under one lock, so two callbacks with one state give
//! one `complete` that continues and one `UnknownState`. It is removed
//! before the token request, so a code is sent at most once.
//!
//! # Session
//!
//! Owned by `session.rs`, keyed by the SHA-256 of the session secret. It
//! holds the principal and the expiry.
//!
//! | State | Event | Guard | Next state |
//! | --- | --- | --- | --- |
//! | absent | `complete` succeeds | every check passed, and fewer than 1,024 sessions are live after expired ones are removed | live |
//! | live | `authenticate` | the expiry is in the future | live |
//! | live | `authenticate` after the expiry | none | expired (removed) |
//! | live | `sign_out` | none | signed out (removed) |
//!
//! No event extends a session. The secret is returned once, in
//! [`SignedIn`], and the crate keeps only its hash.
//!
//! # Key set
//!
//! Owned by `keys.rs`: the checked keys and the time they were fetched.
//!
//! | State | Event | Guard | Next state |
//! | --- | --- | --- | --- |
//! | none | `connect` fetches the set | at least one usable RS256 key | fresh |
//! | none | `connect` fetches an unusable set, or the fetch fails | none | none; `connect` fails |
//! | fresh | one hour passes | none | stale |
//! | fresh | a token names an unknown key id | the last such fetch was at least 60 seconds ago; the fetched set is usable | fresh (replaced) |
//! | fresh | that fetch fails or returns an unusable set | none | fresh (unchanged); the sign-in is refused |
//! | stale | the next verification fetches the set | the fetched set is usable | fresh (replaced) |
//! | stale | that fetch fails or returns an unusable set | none | stale; the sign-in is refused |
//!
//! A key is usable when it is RSA, its `use` is absent or `sig`, its `alg`
//! is absent or RS256, and it has a key id. A fetched set replaces the
//! cached one only when it holds a usable key.
//!
//! # The client secret
//!
//! `connect` reads the secret once through a [`ClientSecretSource`] and
//! holds it as a [`secrecy::SecretString`]. It leaves the process only in
//! the body of the POST to the token endpoint. [`KeychainSecret`] reads it
//! from the macOS Keychain, and [`FixedSecret`] holds one the caller has.
//! No `Debug` or `Display` output of the crate carries a secret, code,
//! token or email.

mod allow;
mod authenticator;
mod clock;
mod config;
mod flow;
mod keychain;
mod keys;
mod provider;
mod refusal;
mod secret;
mod session;
mod token;

pub use allow::{AllowEntry, AllowList, Email, Reach};
pub use authenticator::OidcAuthenticator;
#[cfg(feature = "testing")]
pub use clock::TestClock;
pub use clock::{Clock, SystemClock};
pub use config::{ClientId, ConfigError, IssuerUrl, OidcConfig, RedirectUri};
pub use flow::{KeptState, ReturnTo, SignInStart};
pub use keychain::KeychainSecret;
pub use refusal::{ConnectError, MetadataFault, SecretError, SignInRefusal, TokenFault};
pub use secret::{ClientSecretSource, FixedSecret};
pub use session::SignedIn;

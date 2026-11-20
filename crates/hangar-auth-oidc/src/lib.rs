//! Signs people in to hangar with an OpenID Connect provider, such as
//! Google, and implements [`hangar_core::Authenticator`] for the sessions it
//! creates.
//!
//! [`OidcAuthenticator::connect`] reads the client secret, draws the key that
//! seals pending sign-ins, fetches the provider's discovery document and key
//! set, and checks both. [`OidcAuthenticator::start`] returns the provider
//! URL to send the browser to and the sealed state for the server to keep in
//! a cookie. [`OidcAuthenticator::complete`] takes the callback's query, the
//! kept state and the browser's current session, if any, exchanges the code
//! with PKCE, verifies the ID token, checks the account against the
//! configured [`Admit`] rule and creates a session. The crate holds no HTTP
//! routes, cookies or pages; the server that hosts it does.
//!
//! The crate accepts only ID tokens signed with RS256, asks for no refresh
//! token and stores no ID token or access token. Its sessions and its
//! sealing key live in the memory of one process, so a restart signs
//! everyone out, voids every pending sign-in and leaves nothing to repair.
//! Two processes behind one origin each refuse the other's kept values and
//! sessions, so a server that hosts the crate runs as one process.
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
//! 5. `nonce` equals the sealed pending sign-in's nonce.
//! 6. `email` is present and `email_verified` is the JSON boolean `true`.
//! 7. Under [`Admit::Emails`], the email is ASCII, and lower-cased it is on
//!    the allow-list, whose entry gives the principal. The `hd` claim is
//!    ignored.
//! 8. Under [`Admit::Domain`], in place of step 7: the ID token has an `hd`
//!    claim ([`SignInRefusal::NoHostedDomain`] when it has none); `hd` is a
//!    JSON string equal to the configured domain without regard to ASCII
//!    letter case, as a whole string with nothing trimmed and no subdomain,
//!    parent domain or suffix matching
//!    ([`SignInRefusal::OtherHostedDomain`] otherwise); and the email is
//!    ASCII, so that lower-cased it is the principal id
//!    ([`SignInRefusal::NotAllowed`] otherwise).
//!
//! Under both rules the session's [`hangar_core::Principal`] carries the
//! verified email in lower case, which shares are decided by. Under
//! [`Admit::Emails`] the principal id is the entry's, which may differ
//! from the email.
//!
//! # Admitting a Google Workspace domain
//!
//! [`Admit::Domain`] admits every verified account of one Google Workspace
//! domain. Only the `hd` claim of the signed ID token decides, because
//! Google sets it to the domain of the account's organisation and leaves it
//! out for a personal account. The email's own domain is never compared: a
//! personal account can have an email that ends in the domain, and an
//! organisation's account can have an email in a secondary domain.
//!
//! The authorisation URL from `start` carries `hd=<domain>`, so Google
//! offers that organisation's accounts. A person can change that parameter,
//! so it admits nobody, and an `hd` in the callback query is ignored.
//!
//! `hd` is a Google claim, and the crate does not check that the issuer is
//! Google: with another issuer, [`Admit::Domain`] trusts that issuer's `hd`.
//! Use it with the Google issuer. A session of an account that leaves the
//! organisation lasts until it expires.
//!
//! A domain principal is on no list, so [`hangar_core::Authenticator::authorize`]
//! allows it exactly while at least one of its sessions is live, and the
//! rule's [`Reach`] covers the resource. It checks each session's expiry
//! under the session lock, so a session that has expired but is not yet
//! removed authorises nobody.
//!
//! # Limits
//!
//! - A pending sign-in opens for 10 minutes after its `start`. The crate
//!   stores no pending sign-in, so starts have no limit and none evicts
//!   another.
//! - At most 1,024 sessions are live, each for 12 hours unless
//!   [`OidcConfig::with_session_lifetime`] sets another lifetime. A sign-in
//!   at the limit, after expired sessions, the browser's old session and
//!   the principal's earliest-expiring session beyond 8 are removed, gets
//!   [`SignInRefusal::TooManySessions`].
//! - One principal holds at most 8 live sessions. Its next sign-in ends the
//!   one with the earliest expiry.
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
//! Owned by `flow.rs`. `start` seals the issue time, state, nonce, PKCE
//! verifier and return-to value with AES-256-GCM into the kept value, which
//! the server keeps in a cookie. The key is 32 random bytes drawn at
//! `connect` and held only in memory; the associated data is a fixed label
//! and the client ID; each nonce is four zero bytes and a per-process
//! counter, so no nonce repeats under the key. Nothing is stored
//! server-side.
//!
//! | State | Event | Guard | Next state |
//! | --- | --- | --- | --- |
//! | absent | `start` | none | sealed (in the browser) |
//! | sealed | `complete` with the kept value | it opens under this process's key and client ID, was issued under 10 minutes ago and at most 60 seconds ahead, and its state equals the query's, compared in constant time | opened |
//! | sealed | `complete` with a value that does not open, is expired or is ahead | none | refused as [`SignInRefusal::UnknownState`] |
//! | sealed | `complete` with a value whose state differs | none | refused as [`SignInRefusal::WrongState`] |
//! | sealed | the process restarts | none | void; it no longer opens |
//! | opened | the token exchange and ID token checks | none | a session, or a refusal |
//!
//! A kept value opens again until it is 10 minutes old. A second callback
//! with it sends the code to the provider again, and a provider refuses a
//! second use of a code, so the second callback is
//! [`SignInRefusal::CodeRejected`]. Only the browser that holds the cookie
//! can present the value, and the PKCE verifier and nonce bind the code to
//! it.
//!
//! # Session
//!
//! Owned by `session.rs`, keyed by the SHA-256 of the session secret. It
//! holds the principal and the expiry.
//!
//! | State | Event | Guard | Next state |
//! | --- | --- | --- | --- |
//! | absent | `complete` succeeds under [`Admit::Emails`] | every ID token check passed, the email is verified and on the allow-list, and fewer than 1,024 sessions are live after expired ones, the old session and the principal's evicted session are removed | live |
//! | absent | `complete` succeeds under [`Admit::Domain`] | every ID token check passed, the email is verified, the signed `hd` claim is a string equal to the configured domain without regard to ASCII letter case, the lower-cased email is a valid principal id, and fewer than 1,024 sessions are live after expired ones, the old session and the principal's evicted session are removed | live |
//! | absent | `complete` under [`Admit::Domain`] with no `hd`, another `hd` or an unverified email | none | absent; the sign-in is refused |
//! | live | `authenticate` | the expiry is in the future | live |
//! | live | `authenticate` after the expiry | none | expired (removed) |
//! | live | `sign_out` | none | signed out (removed) |
//! | live | `complete` succeeds with it as the old session | every check of that sign-in passed | replaced (removed in the step that creates the new session) |
//! | live | its principal's 9th sign-in | it has the earliest expiry of the principal's 8 | evicted (removed) |
//!
//! `create` draws the new secret, removes expired sessions, then removes
//! the old session, then the principal's earliest-expiring session while
//! it holds 8, and refuses only when 1,024 sessions remain. A refusal at
//! any step removes nothing that is live. No event extends a session. The secret is returned once, in
//! [`SignedIn`], and the crate keeps only its hash.
//!
//! Under [`Admit::Domain`], a session exists only for an email that the
//! provider's signed ID token marks verified and whose signed `hd` claim
//! equals the configured domain. An email that ends in the domain is never
//! admitted for that reason, and the `hd` of the authorisation request or
//! of the callback query never takes part in the decision.
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
//! from the macOS Keychain, [`FileSecret`] reads it from a file such as a
//! container's secret mount, and [`FixedSecret`] holds one the caller has.
//! No `Debug` or `Display` output of the crate carries a secret, code,
//! token, email or `hd` value. [`SignedIn`]'s `Debug` output hides the
//! principal, which under [`Admit::Domain`] is the person's email.

mod allow;
mod authenticator;
mod clock;
mod config;
mod file_secret;
mod flow;
mod keychain;
mod keys;
mod provider;
mod refusal;
mod secret;
mod session;
mod token;

pub use allow::{Admit, AllowEntry, AllowList, DomainRule, Reach, WorkspaceDomain};
pub use authenticator::OidcAuthenticator;
#[cfg(feature = "testing")]
pub use clock::TestClock;
pub use clock::{Clock, SystemClock};
pub use config::{ClientId, ConfigError, IssuerUrl, OidcConfig, RedirectUri};
pub use file_secret::FileSecret;
pub use flow::{KeptState, ReturnTo, SignInStart};
/// The verified address that sign-in admits, defined in `hangar-core`.
pub use hangar_core::Email;
pub use keychain::KeychainSecret;
pub use refusal::{ConnectError, MetadataFault, SecretError, SignInRefusal, TokenFault};
pub use secret::{ClientSecretSource, FixedSecret};
pub use session::SignedIn;

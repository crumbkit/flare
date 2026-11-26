//! Signs people in to flare with an OpenID Connect provider, such as
//! Google, and implements [`flare_core::Authenticator`] for the sessions it
//! creates.
//!
//! [`OidcAuthenticator::connect`] reads the client secret, fetches the
//! provider's discovery document and key set, and checks both. [`OidcAuthenticator::start`] returns the provider
//! URL to send the browser to and the sealed state for the server to keep in
//! a cookie. [`OidcAuthenticator::complete`] takes the callback's query, the
//! kept state and the browser's current session, if any, exchanges the code
//! with PKCE, verifies the ID token, checks the account against the
//! configured [`Admit`] rule and creates a session. The crate holds no HTTP
//! routes, cookies or pages; the server that hosts it does.
//!
//! [`ProxyAuthenticator`] is the second way in: it trusts the token that a
//! single sign-on proxy in front of flare signs and sets as a cookie, and
//! the same token as a bearer, with the checks of "Proxy token checks"
//! below. It creates no session.
//!
//! The OpenID Connect sign-in accepts only ID tokens signed with RS256, asks for no refresh
//! token and stores no ID token or access token. Sessions and the keys
//! that seal pending sign-ins live in two stores that the caller passes to
//! [`OidcAuthenticator::connect_stored`]. [`OidcAuthenticator::connect`]
//! uses the in-memory stores of `flare-core`, so a restart signs everyone
//! out and two processes refuse each other's sessions and kept values. A
//! site that runs more than one process passes stores that every process
//! shares, such as the DynamoDB stores of `flare-aws`.
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
//! 9. Under [`Admit::Anyone`], in place of step 7: the email is ASCII, and
//!    lower-cased it is the principal id ([`SignInRefusal::NotAllowed`]
//!    otherwise). Any account with a verified email signs in, and reaches
//!    the pages the rule's reach names.
//!
//! Under every rule the session's [`flare_core::Principal`] carries the
//! verified email in lower case, which shares are decided by. Under
//! [`Admit::Emails`] the principal id is the entry's, which may differ
//! from the email.
//!
//! # Proxy token checks, in order
//!
//! 1. The token is at most 8 KiB, or [`ProxyRefusal::TooLong`] before
//!    anything is decoded.
//! 2. The header's `alg` is one that [`ProxyAlgorithms`] lists, RS256,
//!    ES256 or both. Any other value, `none` and every HMAC algorithm
//!    included, is refused before a key is looked up.
//! 3. The header's `kid` names a key in the cached key set, fetched from
//!    the configured https address. For an unknown `kid` the set is
//!    fetched again, at most once every 60 seconds, as for the provider's
//!    key set.
//! 4. The key's family matches the algorithm, and the signature verifies
//!    with that key under that algorithm alone.
//! 5. `iss` equals the configured issuer exactly, and `aud`, a string or a
//!    list, contains the configured audience, so a token for another
//!    application behind the same proxy is refused.
//! 6. `exp` is present and has not passed, and `nbf` and `iat`, when
//!    present, are not in the future, each with 60 seconds of leeway.
//! 7. The identity claim, `email` unless [`IdentityClaim`] names another,
//!    holds an email address, and `email_verified`, when present, is the
//!    JSON boolean `true`.
//! 8. The admit rule admits the address. Under [`Admit::Domain`] the
//!    address's own domain is compared with the rule's, exactly and
//!    without regard to ASCII letter case, because a proxy's token has no
//!    `hd` claim.
//!
//! The proxy's key set keeps RSA keys for RS256 and P-256 keys for ES256,
//! as the configured algorithms allow, and a set of more than 20 keys is
//! unusable. Its states are those of the provider's key set below.
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
//! A domain principal is on no list, so [`flare_core::Authenticator::authorize`]
//! allows it exactly while at least one of its sessions is live, and the
//! rule's [`Reach`] covers the resource. The store compares each session's
//! expiry with the time of the request, so a session that has expired but
//! is not yet removed authorises nobody.
//!
//! # Limits
//!
//! - A pending sign-in opens for 10 minutes after its `start`. The crate
//!   stores no pending sign-in, so starts have no limit and none evicts
//!   another.
//! - A session expires 12 hours after the request that last extended it,
//!   unless [`OidcConfig::with_session_lifetime`] sets another idle life,
//!   and never later than 7 days after sign-in. A request extends a
//!   session only when 10 minutes of its idle life are used. The in-memory
//!   store holds at most 1,024 sessions, and a sign-in at that limit, after
//!   expired sessions, the browser's old session and the principal's
//!   earliest-expiring session beyond 8 are removed, gets
//!   [`SignInRefusal::TooManySessions`]. The DynamoDB store has no such
//!   limit.
//! - A store that cannot answer gives [`SignInRefusal::StoreUnavailable`]
//!   to a start or a callback and [`flare_core::AuthError::Unavailable`] to
//!   a request.
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
//! the server keeps in a cookie. The key is one of the numbered keys of the
//! key store: 32 random bytes, replaced after 30 days, and readable for an
//! hour after its replacement, so a sign-in that began under it finishes.
//! The kept value is the key's number, a dot and the base64url of a random
//! 12-byte nonce followed by the ciphertext. The associated data is a fixed
//! label and the client ID. A process keeps a key it read for 5 minutes,
//! and a kept value that names an unknown key costs one read of the store
//! before it is refused. Nothing else is stored server-side.
//!
//! | State | Event | Guard | Next state |
//! | --- | --- | --- | --- |
//! | absent | `start` | none | sealed (in the browser) |
//! | sealed | `complete` with the kept value | it opens under its numbered key and the client ID, was issued under 10 minutes ago and at most 60 seconds ahead, and its state equals the query's, compared in constant time | opened |
//! | sealed | `complete` with a value that does not open, is expired or is ahead | none | refused as [`SignInRefusal::UnknownState`] |
//! | sealed | `complete` with a value whose state differs | none | refused as [`SignInRefusal::WrongState`] |
//! | sealed | the key's grace ends | none | void; it no longer opens |
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
//! Owned by `session.rs`, which passes each state change to the session
//! store, keyed by the SHA-256 of the session secret. The store holds the
//! principal, the creation time and the expiry.
//!
//! | State | Event | Guard | Next state |
//! | --- | --- | --- | --- |
//! | absent | `complete` succeeds under [`Admit::Emails`] | every ID token check passed, the email is verified and on the allow-list, and fewer than 1,024 sessions are live after expired ones, the old session and the principal's evicted session are removed | live |
//! | absent | `complete` succeeds under [`Admit::Domain`] | every ID token check passed, the email is verified, the signed `hd` claim is a string equal to the configured domain without regard to ASCII letter case, the lower-cased email is a valid principal id, and fewer than 1,024 sessions are live after expired ones, the old session and the principal's evicted session are removed | live |
//! | absent | `complete` under [`Admit::Domain`] with no `hd`, another `hd` or an unverified email | none | absent; the sign-in is refused |
//! | live | `authenticate` | the expiry is in the future | live; the expiry moves to 12 hours ahead, no later than 7 days after sign-in, when 10 minutes of the idle life are used |
//! | live | `authenticate` after the expiry | none | expired (removed) |
//! | live | `sign_out` | none | signed out (removed) |
//! | live | `complete` succeeds with it as the old session | every check of that sign-in passed | replaced (removed in the step that creates the new session) |
//! | live | its principal's 9th sign-in | it has the earliest expiry of the principal's 8 | evicted (removed) |
//!
//! `create` draws the new secret and asks the store to create the session
//! in one step that also ends the old session and the principal's
//! earliest-expiring session while it holds 8. An error leaves every live
//! session as it was. The secret is returned once, in [`SignedIn`], and the
//! store keeps only its hash.
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
mod proxy;
mod refusal;
mod seal;
mod secret;
mod session;
mod stores;
mod token;

pub use allow::{Admit, AllowEntry, AllowList, DomainRule, Reach, WorkspaceDomain};
pub use authenticator::OidcAuthenticator;
#[cfg(feature = "testing")]
pub use clock::TestClock;
pub use clock::{Clock, SystemClock};
pub use config::{ClientId, ConfigError, IssuerUrl, OidcConfig, RedirectUri};
pub use file_secret::FileSecret;
/// The verified address that sign-in admits, defined in `flare-core`.
pub use flare_core::Email;
pub use flow::{KeptState, ReturnTo, SignInStart};
pub use keychain::KeychainSecret;
pub use proxy::{
    Audience, CookieName, IdentityClaim, KeySetUrl, ProxyAlgorithm, ProxyAlgorithms,
    ProxyAuthenticator, ProxyConfig, ProxyConfigError, ProxyIssuer, ProxyLink,
};
pub use refusal::{
    ConnectError, MetadataFault, ProxyConnectError, ProxyRefusal, SecretError, SignInRefusal,
    TokenFault,
};
pub use secret::{ClientSecretSource, FixedSecret};
pub use session::SignedIn;

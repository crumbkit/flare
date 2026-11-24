//! flare-server driven through a real listener on a free port, with
//! memory stores, a raw HTTP/1.1 client and, for OpenID Connect, the fake
//! provider of flare-auth-oidc's tests.
#![cfg(test)]

#[path = "../../flare-auth-oidc/tests/support/fake_provider.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "the fake provider is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod fake_provider;

#[path = "../../flare-auth-loopback/tests/support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch directory is flare-auth-loopback's test support, which panics on a broken fixture"
)]
pub mod scratch;

#[path = "support/harness.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the harness is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod harness;

#[path = "support/hosted.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the hosted harness is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod hosted;

#[path = "support/faulty.rs"]
pub mod faulty;

#[path = "support/api.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the API harness is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod api;

#[path = "server/api.rs"]
mod api_routes;
#[path = "server/authenticators.rs"]
mod authenticators;
#[path = "server/crashes.rs"]
mod crashes;
#[path = "server/edits.rs"]
mod edits;
#[path = "server/forms.rs"]
mod forms;
#[path = "server/headers.rs"]
mod headers;
#[path = "server/hosted_sign_in.rs"]
mod hosted_sign_in;
#[path = "server/hosted.rs"]
mod hosted_site;
#[path = "server/limits.rs"]
mod limits;
#[path = "server/loopback.rs"]
mod loopback;
#[path = "server/oidc.rs"]
mod oidc;
#[path = "server/ops.rs"]
mod ops;
#[path = "server/pages.rs"]
mod pages;
#[path = "server/process.rs"]
mod process;
#[path = "server/search.rs"]
mod search;
#[path = "server/versions.rs"]
mod versions;

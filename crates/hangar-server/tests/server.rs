//! hangar-server driven through a real listener on a free port, with
//! memory stores, a raw HTTP/1.1 client and, for OpenID Connect, the fake
//! provider of hangar-auth-oidc's tests.
#![cfg(test)]

#[path = "../../hangar-auth-oidc/tests/support/fake_provider.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "the fake provider is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod fake_provider;

#[path = "../../hangar-auth-loopback/tests/support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch directory is hangar-auth-loopback's test support, which panics on a broken fixture"
)]
pub mod scratch;

#[path = "support/harness.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the harness is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod harness;

#[path = "server/authenticators.rs"]
mod authenticators;
#[path = "server/edits.rs"]
mod edits;
#[path = "server/forms.rs"]
mod forms;
#[path = "server/headers.rs"]
mod headers;
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

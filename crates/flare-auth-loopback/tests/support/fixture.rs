//! The servers and requests that the tests build.

use flare_auth_loopback::{Loopback, Served, local_principal};
use flare_core::{Credential, RecordPath};
use std::time::Instant;

/// The port of every test server.
pub const PORT: u16 = 4100;

/// A server for the local user `ada` that serves everything.
pub fn loopback() -> Loopback {
    loopback_serving(Served::Everything)
}

/// A server for the local user `ada` that serves `served`.
pub fn loopback_serving(served: Served) -> Loopback {
    Loopback::new(PORT, local_principal("ada").unwrap(), served).unwrap()
}

/// What the contract tests serve: the records under `/tasks`.
pub fn tasks_only() -> Served {
    Served::RecordsUnder(vec![RecordPath::try_from("/tasks").unwrap()])
}

/// The bearer token of `loopback`.
pub fn bearer(loopback: &Loopback) -> Credential {
    Credential::Bearer(loopback.server_record().token().clone())
}

/// Signs in through a fresh launch code and returns the session
/// credential.
pub fn session(loopback: &Loopback, now: Instant) -> Credential {
    use secrecy::ExposeSecret as _;
    let code = loopback
        .issue_code(flare_auth_loopback::ReturnPath::try_from("/").unwrap(), now)
        .unwrap();
    loopback
        .sign_in(code.secret().expose_secret(), now)
        .unwrap()
        .session()
        .clone()
}

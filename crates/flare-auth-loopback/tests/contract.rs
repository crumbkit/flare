//! Runs flare-core's Authenticator contract suite against a loopback
//! server, once with its bearer token and once with a session secret.
#![cfg(test)]

#[path = "support/fixture.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the fixture is test support: it panics on a broken fixture, and each test file uses part of it"
)]
pub mod fixture;

use fixture::{bearer, loopback_serving, session, tasks_only};
use flare_core::contract::authenticator::{self, AuthFixture};
use flare_core::contract::block_on;
use flare_core::{Credential, RecordPath, Resource};
use std::time::Instant;

fn resources() -> (Resource, Resource) {
    (
        Resource::Record(RecordPath::try_from("/tasks/a").unwrap()),
        Resource::Record(RecordPath::try_from("/orders/a").unwrap()),
    )
}

macro_rules! authenticator_suite {
    ($credential:ident: $($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            let loopback = loopback_serving(tasks_only());
            let accepted = $credential(&loopback);
            let (reachable, unreachable) = resources();
            let fixture = AuthFixture::new(
                accepted,
                Credential::bearer("0".repeat(64)),
                reachable,
                unreachable,
            );
            block_on(authenticator::$name(&loopback, &fixture)).unwrap();
        }
    )*};
}

mod with_the_bearer_token {
    use super::*;

    authenticator_suite!(
        bearer: accepted_credential_gives_the_same_principal,
        rejected_credential_is_rejected,
        access_follows_the_resource,
    );
}

mod with_a_session_secret {
    use super::*;

    fn signed_in(loopback: &flare_auth_loopback::Loopback) -> Credential {
        session(loopback, Instant::now())
    }

    authenticator_suite!(
        signed_in: accepted_credential_gives_the_same_principal,
        rejected_credential_is_rejected,
        access_follows_the_resource,
    );
}

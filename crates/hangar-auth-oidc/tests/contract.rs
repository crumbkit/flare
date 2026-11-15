//! Runs hangar-core's Authenticator contract suite against an authenticator
//! that signed a person in through the fake provider.
#![cfg(test)]

#[path = "support/fake_provider.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "the fake provider is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod fake_provider;

use fake_provider::{Fake, HOME, connect, sign_in};
use hangar_core::contract::authenticator::{self, AuthFixture};
use hangar_core::contract::block_on;
use hangar_core::{Credential, PageName, Resource};

macro_rules! authenticator_suite {
    ($($name:ident),* $(,)?) => {$(
        #[test]
        fn $name() {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let (authenticator, signed_in) = runtime.block_on(async {
                let fake = Fake::start().await;
                let (authenticator, _clock) = connect(&fake).await;
                let signed_in = sign_in(&authenticator, &fake).await.unwrap();
                (authenticator, signed_in)
            });
            let fixture = AuthFixture::new(
                signed_in.session().clone(),
                Credential::session("an-unknown-session-secret"),
                Resource::Page(PageName::try_from(HOME).unwrap()),
                Resource::Page(PageName::try_from("elsewhere").unwrap()),
            );
            block_on(authenticator::$name(&authenticator, &fixture)).unwrap();
        }
    )*};
}

authenticator_suite!(
    accepted_credential_gives_the_same_principal,
    rejected_credential_is_rejected,
    access_follows_the_resource,
);

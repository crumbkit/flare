//! Sessions after a sign-in: which credentials `authenticate` accepts, their
//! expiry, sign-out, the session limit and `authorize`.
#![cfg(test)]

#[path = "support/fake_provider.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "the fake provider is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod fake_provider;

use fake_provider::{EMAIL, Fake, HOME, assert_refused, config_with, connect, secret, sign_in};
use hangar_auth_oidc::{
    AllowEntry, AllowList, Email, OidcAuthenticator, Reach, SignInRefusal, TestClock,
};
use hangar_core::{
    Access, AuthError, Authenticator, Credential, Decision, PageName, Principal, PrincipalId,
    Resource,
};
use std::time::Duration;

fn page(name: &str) -> Resource {
    Resource::Page(PageName::try_from(name).unwrap())
}

#[tokio::test]
async fn a_bearer_credential_is_rejected_and_the_same_secret_as_a_session_is_accepted() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let signed_in = sign_in(&authenticator, &fake).await.unwrap();
    let Credential::Session(secret) = signed_in.session() else {
        panic!("a session credential");
    };
    let bearer = Credential::Bearer(secret.clone());
    assert_eq!(
        authenticator.authenticate(&bearer).await,
        Err(AuthError::Rejected)
    );
    assert!(
        authenticator
            .authenticate(signed_in.session())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_session_is_rejected_after_twelve_hours_and_accepted_a_second_before() {
    let fake = Fake::start().await;
    let (authenticator, clock) = connect(&fake).await;
    let signed_in = sign_in(&authenticator, &fake).await.unwrap();
    clock.advance(Duration::from_secs(12 * 60 * 60 - 1));
    assert!(
        authenticator
            .authenticate(signed_in.session())
            .await
            .is_ok()
    );
    clock.advance(Duration::from_secs(1));
    assert_eq!(
        authenticator.authenticate(signed_in.session()).await,
        Err(AuthError::Rejected)
    );
}

#[tokio::test]
async fn sign_out_rejects_that_session_and_another_session_stays_accepted() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let first = sign_in(&authenticator, &fake).await.unwrap();
    let second = sign_in(&authenticator, &fake).await.unwrap();
    authenticator.sign_out(first.session());
    assert_eq!(
        authenticator.authenticate(first.session()).await,
        Err(AuthError::Rejected)
    );
    assert!(authenticator.authenticate(second.session()).await.is_ok());
}

#[tokio::test]
async fn the_1025th_sign_in_is_refused_and_succeeds_after_an_older_session_expires() {
    let fake = Fake::start().await;
    let (authenticator, clock) = connect(&fake).await;
    assert!(sign_in(&authenticator, &fake).await.is_ok());
    clock.advance(Duration::from_secs(60));
    for _ in 1..1024 {
        assert!(sign_in(&authenticator, &fake).await.is_ok());
    }
    let refusal = sign_in(&authenticator, &fake).await.unwrap_err();
    assert_eq!(refusal, SignInRefusal::TooManySessions);
    assert!(refusal.is_temporary());
    // The first session expires sixty seconds before the others.
    clock.advance(Duration::from_secs(12 * 60 * 60 - 30));
    assert!(sign_in(&authenticator, &fake).await.is_ok());
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::TooManySessions,
    );
}

async fn two_entry_authenticator(fake: &Fake) -> OidcAuthenticator<TestClock> {
    let allow = AllowList::new(vec![
        AllowEntry::new(
            Email::try_from(EMAIL).unwrap(),
            PrincipalId::try_from("ada").unwrap(),
            Reach::Only(vec![page(HOME)]),
        ),
        AllowEntry::new(
            Email::try_from("grace@example.com").unwrap(),
            PrincipalId::try_from("grace").unwrap(),
            Reach::Everything,
        ),
    ])
    .unwrap();
    OidcAuthenticator::connect_with_clock(config_with(fake, allow), &secret(), TestClock::new())
        .await
        .unwrap()
}

#[tokio::test]
async fn authorize_denies_a_principal_off_the_allow_list_and_allows_everything_its_reach_names() {
    let fake = Fake::start().await;
    let authenticator = two_entry_authenticator(&fake).await;
    let stranger = Principal::new(PrincipalId::try_from("mallory").unwrap());
    let ada = Principal::new(PrincipalId::try_from("ada").unwrap());
    let grace = Principal::new(PrincipalId::try_from("grace").unwrap());
    for access in [Access::Read, Access::Write] {
        let ask = |principal: &Principal, resource: Resource| {
            let authenticator = &authenticator;
            let principal = principal.clone();
            async move { authenticator.authorize(&principal, access, &resource).await }
        };
        assert_eq!(ask(&stranger, page(HOME)).await, Ok(Decision::Denied));
        assert_eq!(ask(&ada, page(HOME)).await, Ok(Decision::Allowed));
        assert_eq!(ask(&ada, page("elsewhere")).await, Ok(Decision::Denied));
        assert_eq!(ask(&grace, page("elsewhere")).await, Ok(Decision::Allowed));
    }
}

#[tokio::test]
async fn a_second_allow_listed_person_signs_in_as_their_own_principal() {
    let fake = Fake::start().await;
    let authenticator = two_entry_authenticator(&fake).await;
    fake.set_email("grace@example.com");
    let start = authenticator.start(fake_provider::return_to()).unwrap();
    let query = fake.approve(start.redirect());
    let signed_in = authenticator
        .complete(&query, Some(start.kept()))
        .await
        .unwrap();
    assert_eq!(
        signed_in.principal().id(),
        &PrincipalId::try_from("grace").unwrap()
    );
}

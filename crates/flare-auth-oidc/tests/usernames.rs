//! A deployment that names people by username: the OIDC sign-in reads the
//! username from the configured claim and refuses a sign-in without one,
//! and its admit rules still judge the verified email. Every refusal test
//! has a twin that shows the admitted case.
#![cfg(test)]

#[path = "support/fake_provider.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "the fake provider is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod fake_provider;

use fake_provider::{EMAIL, Fake, PRINCIPAL, assert_refused, config, secret, sign_in};
use flare_auth_oidc::{Identity, IdentityClaim, OidcAuthenticator, SignInRefusal, TestClock};
use flare_core::{Authenticator as _, Person};
use serde_json::json;

fn username_claim() -> Identity {
    Identity::Username(IdentityClaim::try_from("preferred_username").unwrap())
}

async fn connect_usernames(fake: &Fake) -> OidcAuthenticator<TestClock> {
    let config = config(fake).with_identity(username_claim());
    OidcAuthenticator::connect_with_clock(config, &secret(), TestClock::new())
        .await
        .expect("connect succeeds against the fake")
}

#[tokio::test]
async fn a_sign_in_with_the_username_claim_names_the_person_by_username_beside_the_email() {
    let fake = Fake::start().await;
    fake.edit_claims(|claims| {
        claims.insert("preferred_username".into(), json!("Ada"));
    });
    let authenticator = connect_usernames(&fake).await;
    let signed_in = sign_in(&authenticator, &fake).await.expect("a sign-in");
    let principal = signed_in.principal();
    assert_eq!(principal.id().as_str(), PRINCIPAL);
    assert_eq!(principal.person(), Some(&Person::try_from("ada").unwrap()));
    assert_eq!(principal.email().unwrap().as_str(), EMAIL);
    let again = authenticator
        .authenticate(signed_in.session())
        .await
        .unwrap();
    assert_eq!(again.person(), principal.person());
}

#[tokio::test]
async fn a_sign_in_without_the_username_claim_is_refused() {
    let fake = Fake::start().await;
    let authenticator = connect_usernames(&fake).await;
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::NoUsername,
    );
}

#[tokio::test]
async fn a_sign_in_whose_username_claim_is_not_a_username_is_refused() {
    let fake = Fake::start().await;
    for bad in [
        json!("ada@example.com"),
        json!("everyone"),
        json!(7),
        json!(""),
    ] {
        fake.edit_claims(move |claims| {
            claims.insert("preferred_username".into(), bad.clone());
        });
        let authenticator = connect_usernames(&fake).await;
        assert_refused(
            sign_in(&authenticator, &fake).await,
            &SignInRefusal::NoUsername,
        );
    }
}

#[tokio::test]
async fn under_the_email_kind_the_username_claim_is_ignored() {
    let fake = Fake::start().await;
    fake.edit_claims(|claims| {
        claims.insert("preferred_username".into(), json!("ada"));
    });
    let config = config(&fake);
    let authenticator = OidcAuthenticator::connect_with_clock(config, &secret(), TestClock::new())
        .await
        .unwrap();
    let signed_in = sign_in(&authenticator, &fake).await.expect("a sign-in");
    assert_eq!(
        signed_in.principal().person(),
        Some(&Person::try_from(EMAIL).unwrap())
    );
}

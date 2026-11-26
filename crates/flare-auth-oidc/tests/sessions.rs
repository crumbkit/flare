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

use fake_provider::{
    DOMAIN, EMAIL, Fake, HOME, TokenReply, assert_refused, config_with, connect, connect_domain,
    secret, sign_in, sign_in_replacing,
};
use flare_auth_oidc::{
    Admit, AllowEntry, AllowList, Email, OidcAuthenticator, Reach, SignInRefusal, SignedIn,
    TestClock,
};
use flare_core::{
    Access, AuthError, Authenticator, Credential, Decision, PageName, Principal, PrincipalId,
    Resource,
};
use serde_json::json;
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
async fn an_unused_session_is_rejected_after_twelve_hours_and_not_before() {
    let fake = Fake::start().await;
    let (authenticator, clock) = connect(&fake).await;
    let signed_in = sign_in(&authenticator, &fake).await.unwrap();
    let other = sign_in(&authenticator, &fake).await.unwrap();
    clock.advance(Duration::from_secs(12 * 60 * 60 - 1));
    assert!(
        authenticator
            .authenticate(signed_in.session())
            .await
            .is_ok()
    );
    clock.advance(Duration::from_secs(1));
    assert_eq!(
        authenticator.authenticate(other.session()).await,
        Err(AuthError::Rejected)
    );
}

#[tokio::test]
async fn a_session_in_use_lives_past_twelve_hours_and_ends_seven_days_after_sign_in() {
    let fake = Fake::start().await;
    let (authenticator, clock) = connect(&fake).await;
    let signed_in = sign_in(&authenticator, &fake).await.unwrap();
    for _ in 0..(7 * 24 * 60 / 30) - 1 {
        clock.advance(Duration::from_secs(30 * 60));
        assert!(
            authenticator
                .authenticate(signed_in.session())
                .await
                .is_ok()
        );
    }
    clock.advance(Duration::from_secs(30 * 60));
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
    authenticator.sign_out(first.session()).await.unwrap();
    assert_eq!(
        authenticator.authenticate(first.session()).await,
        Err(AuthError::Rejected)
    );
    assert!(authenticator.authenticate(second.session()).await.is_ok());
}

/// Signs in the domain account `<name>@example.com`.
async fn sign_in_as(
    authenticator: &OidcAuthenticator<TestClock>,
    fake: &Fake,
    name: &str,
) -> Result<SignedIn, SignInRefusal> {
    fake.set_email(&format!("{name}@{DOMAIN}"));
    sign_in(authenticator, fake).await
}

/// Fills the table with `count` sessions of distinct domain accounts.
async fn fill(authenticator: &OidcAuthenticator<TestClock>, fake: &Fake, count: usize) {
    for at in 0..count {
        assert!(
            sign_in_as(authenticator, fake, &format!("filler{at}"))
                .await
                .is_ok()
        );
    }
}

#[tokio::test]
async fn the_1025th_sign_in_is_refused_and_succeeds_after_an_older_session_expires() {
    let fake = Fake::start().await;
    fake.set_hd(Some(json!(DOMAIN)));
    let (authenticator, clock) = connect_domain(&fake).await;
    assert!(sign_in_as(&authenticator, &fake, "first").await.is_ok());
    clock.advance(Duration::from_secs(60));
    fill(&authenticator, &fake, 1023).await;
    let refusal = sign_in_as(&authenticator, &fake, "late").await.unwrap_err();
    assert_eq!(refusal, SignInRefusal::TooManySessions);
    assert!(refusal.is_temporary());
    // The first session expires sixty seconds before the others.
    clock.advance(Duration::from_secs(12 * 60 * 60 - 30));
    assert!(sign_in_as(&authenticator, &fake, "late").await.is_ok());
    assert_refused(
        sign_in_as(&authenticator, &fake, "later").await,
        &SignInRefusal::TooManySessions,
    );
}

#[tokio::test]
async fn a_principals_ninth_sign_in_ends_its_earliest_expiring_session_and_no_one_elses() {
    let fake = Fake::start().await;
    fake.set_hd(Some(json!(DOMAIN)));
    let (authenticator, clock) = connect_domain(&fake).await;
    let grace = sign_in_as(&authenticator, &fake, "grace").await.unwrap();
    let mut ada = Vec::new();
    for _ in 0..8 {
        ada.push(sign_in_as(&authenticator, &fake, "ada").await.unwrap());
        clock.advance(Duration::from_secs(1));
    }
    for signed_in in &ada {
        assert!(
            authenticator
                .authenticate(signed_in.session())
                .await
                .is_ok()
        );
    }
    let ninth = sign_in_as(&authenticator, &fake, "ada").await.unwrap();
    assert_eq!(
        authenticator.authenticate(ada[0].session()).await,
        Err(AuthError::Rejected),
        "the earliest-expiring session ends"
    );
    for signed_in in ada.iter().skip(1).chain([&ninth]) {
        assert!(
            authenticator
                .authenticate(signed_in.session())
                .await
                .is_ok()
        );
    }
    assert!(authenticator.authenticate(grace.session()).await.is_ok());
}

#[tokio::test]
async fn a_sign_in_that_passes_the_old_session_ends_it_and_one_without_keeps_it() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let kept = sign_in(&authenticator, &fake).await.unwrap();
    let old = sign_in(&authenticator, &fake).await.unwrap();
    let new = sign_in_replacing(&authenticator, &fake, Some(old.session()))
        .await
        .unwrap();
    assert_eq!(
        authenticator.authenticate(old.session()).await,
        Err(AuthError::Rejected)
    );
    assert!(authenticator.authenticate(new.session()).await.is_ok());
    assert!(authenticator.authenticate(kept.session()).await.is_ok());
}

#[tokio::test]
async fn passing_the_old_session_removes_it_before_the_table_limit_is_checked() {
    let fake = Fake::start().await;
    fake.set_hd(Some(json!(DOMAIN)));
    let (authenticator, _clock) = connect_domain(&fake).await;
    let old = sign_in_as(&authenticator, &fake, "ada").await.unwrap();
    fill(&authenticator, &fake, 1023).await;
    fake.set_email(&format!("ada@{DOMAIN}"));
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::TooManySessions,
    );
    let new = sign_in_replacing(&authenticator, &fake, Some(old.session()))
        .await
        .unwrap();
    assert!(authenticator.authenticate(new.session()).await.is_ok());
    assert_eq!(
        authenticator.authenticate(old.session()).await,
        Err(AuthError::Rejected)
    );
}

#[tokio::test]
async fn a_principal_at_eight_sessions_signs_in_while_the_table_is_full() {
    let fake = Fake::start().await;
    fake.set_hd(Some(json!(DOMAIN)));
    let (authenticator, _clock) = connect_domain(&fake).await;
    for _ in 0..8 {
        assert!(sign_in_as(&authenticator, &fake, "ada").await.is_ok());
    }
    fill(&authenticator, &fake, 1016).await;
    assert_refused(
        sign_in_as(&authenticator, &fake, "grace").await,
        &SignInRefusal::TooManySessions,
    );
    assert!(sign_in_as(&authenticator, &fake, "ada").await.is_ok());
}

#[tokio::test]
async fn a_failed_sign_in_leaves_the_old_session_live_and_a_good_one_ends_it() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let old = sign_in(&authenticator, &fake).await.unwrap();
    fake.set_email("eve@example.com");
    assert_refused(
        sign_in_replacing(&authenticator, &fake, Some(old.session())).await,
        &SignInRefusal::NotAllowed,
    );
    assert!(authenticator.authenticate(old.session()).await.is_ok());
    fake.token_reply(TokenReply::Status(500));
    fake.set_email(EMAIL);
    assert_refused(
        sign_in_replacing(&authenticator, &fake, Some(old.session())).await,
        &SignInRefusal::ProviderUnavailable,
    );
    assert!(authenticator.authenticate(old.session()).await.is_ok());
    fake.token_reply(TokenReply::Normal);
    assert!(
        sign_in_replacing(&authenticator, &fake, Some(old.session()))
            .await
            .is_ok()
    );
    assert_eq!(
        authenticator.authenticate(old.session()).await,
        Err(AuthError::Rejected)
    );
}

async fn two_entry_authenticator(fake: &Fake) -> OidcAuthenticator<TestClock> {
    let allow = AllowList::new(vec![
        AllowEntry::new(
            Email::try_from(EMAIL).unwrap().into(),
            PrincipalId::try_from("ada").unwrap(),
            Reach::Only(vec![page(HOME)]),
        ),
        AllowEntry::new(
            Email::try_from("grace@example.com").unwrap().into(),
            PrincipalId::try_from("grace").unwrap(),
            Reach::Everything,
        ),
    ])
    .unwrap();
    OidcAuthenticator::connect_with_clock(
        config_with(fake, Admit::Emails(allow)),
        &secret(),
        TestClock::new(),
    )
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
    let start = authenticator
        .start(fake_provider::return_to())
        .await
        .unwrap();
    let query = fake.approve(start.redirect());
    let signed_in = authenticator
        .complete(&query, Some(start.kept()), None)
        .await
        .unwrap();
    assert_eq!(
        signed_in.principal().id(),
        &PrincipalId::try_from("grace").unwrap()
    );
}

fn principal(id: &str) -> Principal {
    Principal::new(PrincipalId::try_from(id).unwrap())
}

async fn ask<C: flare_auth_oidc::Clock>(
    authenticator: &OidcAuthenticator<C>,
    principal: &Principal,
    resource: &Resource,
) -> Decision {
    authenticator
        .authorize(principal, Access::Read, resource)
        .await
        .unwrap()
}

#[tokio::test]
async fn authorize_allows_a_domain_principal_only_while_a_session_of_it_is_live() {
    let fake = Fake::start().await;
    fake.set_hd(Some(json!(DOMAIN)));
    let (authenticator, clock) = connect_domain(&fake).await;
    let ada = principal(EMAIL);
    assert_eq!(
        ask(&authenticator, &ada, &page(HOME)).await,
        Decision::Denied
    );
    let signed_in = sign_in(&authenticator, &fake).await.unwrap();
    assert_eq!(
        ask(&authenticator, &ada, &page(HOME)).await,
        Decision::Allowed
    );
    for access in [Access::Read, Access::Write] {
        assert_eq!(
            authenticator.authorize(&ada, access, &page(HOME)).await,
            Ok(Decision::Allowed)
        );
    }
    // The rule's reach still bounds a principal with a live session.
    assert_eq!(
        ask(&authenticator, &ada, &page("elsewhere")).await,
        Decision::Denied
    );
    authenticator.sign_out(signed_in.session()).await.unwrap();
    assert_eq!(
        ask(&authenticator, &ada, &page(HOME)).await,
        Decision::Denied
    );

    assert!(sign_in(&authenticator, &fake).await.is_ok());
    clock.advance(Duration::from_secs(12 * 60 * 60 - 1));
    assert_eq!(
        ask(&authenticator, &ada, &page(HOME)).await,
        Decision::Allowed
    );
    clock.advance(Duration::from_secs(1));
    assert_eq!(
        ask(&authenticator, &ada, &page(HOME)).await,
        Decision::Denied
    );
}

#[tokio::test]
async fn a_domain_principal_with_two_sessions_stays_allowed_after_one_signs_out() {
    let fake = Fake::start().await;
    fake.set_hd(Some(json!(DOMAIN)));
    let (authenticator, _clock) = connect_domain(&fake).await;
    let ada = principal(EMAIL);
    let first = sign_in(&authenticator, &fake).await.unwrap();
    let second = sign_in(&authenticator, &fake).await.unwrap();
    authenticator.sign_out(first.session()).await.unwrap();
    assert_eq!(
        ask(&authenticator, &ada, &page(HOME)).await,
        Decision::Allowed
    );
    authenticator.sign_out(second.session()).await.unwrap();
    assert_eq!(
        ask(&authenticator, &ada, &page(HOME)).await,
        Decision::Denied
    );
}

#[tokio::test]
async fn authorize_denies_a_principal_of_the_domain_that_never_signed_in() {
    let fake = Fake::start().await;
    fake.set_hd(Some(json!(DOMAIN)));
    let (authenticator, _clock) = connect_domain(&fake).await;
    assert!(sign_in(&authenticator, &fake).await.is_ok());
    assert_eq!(
        ask(&authenticator, &principal("grace@example.com"), &page(HOME)).await,
        Decision::Denied
    );
    assert_eq!(
        ask(&authenticator, &principal(EMAIL), &page(HOME)).await,
        Decision::Allowed
    );
}

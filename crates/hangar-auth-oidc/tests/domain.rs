//! Sign-in under `Admit::Domain` against the fake provider: the signed `hd`
//! claim decides, and neither the email's domain nor any `hd` parameter
//! outside the ID token does.
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
    DOMAIN, EMAIL, Fake, assert_refused, connect, connect_domain, form, return_to, sign_in,
};
use hangar_auth_oidc::{KeptState, SignInRefusal, SignedIn};
use hangar_core::{Authenticator, PrincipalId};
use serde_json::{Value, json};
use url::Url;

/// Signs `email` in under the domain rule, with `hd` as the token's claim.
async fn domain_sign_in(email: &str, hd: Option<Value>) -> Result<SignedIn, SignInRefusal> {
    let fake = Fake::start().await;
    fake.set_email(email);
    fake.set_hd(hd);
    let (authenticator, _clock) = connect_domain(&fake).await;
    sign_in(&authenticator, &fake).await
}

fn principal_of(result: Result<SignedIn, SignInRefusal>) -> String {
    let signed_in = result.expect("the sign-in succeeds");
    signed_in.principal().id().as_str().to_owned()
}

fn hd_parameter(url: &Url) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name == "hd")
        .map(|(_, value)| value.into_owned())
}

#[tokio::test]
async fn a_verified_email_whose_hd_is_the_domain_signs_in_as_the_lower_cased_email() {
    let fake = Fake::start().await;
    fake.set_email("Ada.Lovelace@Example.com");
    fake.set_hd(Some(json!(DOMAIN)));
    let (authenticator, _clock) = connect_domain(&fake).await;
    let signed_in = sign_in(&authenticator, &fake).await.unwrap();
    let expected = PrincipalId::try_from("ada.lovelace@example.com").unwrap();
    assert_eq!(signed_in.principal().id(), &expected);
    let principal = authenticator
        .authenticate(signed_in.session())
        .await
        .unwrap();
    assert_eq!(principal.id(), &expected);
}

#[tokio::test]
async fn a_verified_email_without_an_hd_claim_is_refused_as_no_hosted_domain() {
    assert_refused(
        domain_sign_in(EMAIL, None).await,
        &SignInRefusal::NoHostedDomain,
    );
    // The admitted case: the same account with the claim.
    assert!(domain_sign_in(EMAIL, Some(json!(DOMAIN))).await.is_ok());
}

#[tokio::test]
async fn a_verified_email_whose_hd_names_another_domain_is_refused_as_other_hosted_domain() {
    assert_refused(
        domain_sign_in("ada@other.org", Some(json!("other.org"))).await,
        &SignInRefusal::OtherHostedDomain,
    );
    assert!(
        domain_sign_in("ada@other.org", Some(json!(DOMAIN)))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn an_unverified_or_missing_email_with_the_matching_hd_is_refused_as_email_unverified() {
    for edit in [false, true] {
        let fake = Fake::start().await;
        fake.set_hd(Some(json!(DOMAIN)));
        fake.edit_claims(move |claims| {
            if edit {
                claims.remove("email");
            } else {
                claims.insert("email_verified".into(), json!(false));
            }
        });
        let (authenticator, _clock) = connect_domain(&fake).await;
        assert_refused(
            sign_in(&authenticator, &fake).await,
            &SignInRefusal::EmailUnverified,
        );
    }
    // An unverified email is refused for that reason whatever `hd` holds.
    let fake = Fake::start().await;
    fake.edit_claims(|claims| {
        claims.insert("email_verified".into(), json!(false));
    });
    let (authenticator, _clock) = connect_domain(&fake).await;
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::EmailUnverified,
    );
    assert!(domain_sign_in(EMAIL, Some(json!(DOMAIN))).await.is_ok());
}

#[tokio::test]
async fn an_email_that_ends_in_the_domain_is_refused_without_the_matching_hd() {
    assert!(EMAIL.ends_with(&format!("@{DOMAIN}")));
    assert_refused(
        domain_sign_in(EMAIL, None).await,
        &SignInRefusal::NoHostedDomain,
    );
    assert_refused(
        domain_sign_in(EMAIL, Some(json!("gmail.com"))).await,
        &SignInRefusal::OtherHostedDomain,
    );
    assert!(domain_sign_in(EMAIL, Some(json!(DOMAIN))).await.is_ok());
}

#[tokio::test]
async fn an_hd_in_another_letter_case_is_admitted() {
    assert_eq!(
        principal_of(domain_sign_in(EMAIL, Some(json!("Example.COM"))).await),
        EMAIL
    );
}

#[tokio::test]
async fn an_hd_that_is_not_the_whole_domain_or_not_a_string_is_refused_as_other_hosted_domain() {
    for hd in [
        json!("eu.example.com"),
        json!("com"),
        json!("notexample.com"),
        json!("example.com.evil.test"),
        json!("example.com."),
        json!(" example.com"),
        json!(""),
        json!(42),
        json!(null),
        json!([DOMAIN]),
    ] {
        assert_refused(
            domain_sign_in(EMAIL, Some(hd.clone())).await,
            &SignInRefusal::OtherHostedDomain,
        );
    }
    assert!(domain_sign_in(EMAIL, Some(json!(DOMAIN))).await.is_ok());
}

#[tokio::test]
async fn a_matching_hd_with_an_email_in_another_domain_is_admitted() {
    assert_eq!(
        principal_of(domain_sign_in("ada@secondary.example.net", Some(json!(DOMAIN))).await),
        "ada@secondary.example.net"
    );
}

#[tokio::test]
async fn a_matching_hd_with_an_email_that_cannot_be_a_principal_id_is_refused_as_not_allowed() {
    assert_refused(
        domain_sign_in("j\u{00F6}rg@example.com", Some(json!(DOMAIN))).await,
        &SignInRefusal::NotAllowed,
    );
    assert!(
        domain_sign_in("jorg@example.com", Some(json!(DOMAIN)))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn the_authorisation_url_carries_the_domain_as_hd_only_under_the_domain_rule() {
    let fake = Fake::start().await;
    let (domain, _clock) = connect_domain(&fake).await;
    let start = domain.start(return_to()).unwrap();
    assert_eq!(hd_parameter(start.redirect()).as_deref(), Some(DOMAIN));
    let (emails, _clock) = connect(&fake).await;
    let start = emails.start(return_to()).unwrap();
    assert_eq!(hd_parameter(start.redirect()), None);
}

#[tokio::test]
async fn under_the_allow_list_hd_is_ignored_and_an_unlisted_email_is_refused_as_not_allowed() {
    for hd in [
        None,
        Some(json!(DOMAIN)),
        Some(json!("other.org")),
        Some(json!(42)),
    ] {
        let fake = Fake::start().await;
        fake.set_hd(hd.clone());
        let (authenticator, _clock) = connect(&fake).await;
        assert!(sign_in(&authenticator, &fake).await.is_ok(), "{hd:?}");
        fake.set_email("mallory@example.com");
        assert_refused(
            sign_in(&authenticator, &fake).await,
            &SignInRefusal::NotAllowed,
        );
    }
}

#[tokio::test]
async fn an_hd_parameter_in_the_callback_query_does_not_admit_a_token_without_the_claim() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect_domain(&fake).await;
    for (claim, admitted) in [(None, false), (Some(json!(DOMAIN)), true)] {
        fake.set_hd(claim);
        let start = authenticator.start(return_to()).unwrap();
        let query = format!(
            "{}&{}",
            fake.approve(start.redirect()),
            form(&[("hd", DOMAIN)])
        );
        let kept = KeptState::from_cookie(start.kept().expose());
        let result = authenticator.complete(&query, Some(&kept), None).await;
        if admitted {
            assert!(result.is_ok(), "{result:?}");
        } else {
            assert_refused(result, &SignInRefusal::NoHostedDomain);
        }
    }
}

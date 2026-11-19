//! Sign-ins against the fake provider: the good path and each refusal, with
//! the admitted case beside each refusal.
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
    CLIENT_ID, CLIENT_SECRET, EMAIL, Fake, Forge, PRINCIPAL, REDIRECT, TokenReply, assert_refused,
    config_with, connect, now_seconds, return_to, secret, sign_in,
};
use hangar_auth_oidc::{
    Admit, AllowEntry, AllowList, Email, KeptState, OidcAuthenticator, Reach, SignInRefusal,
    TestClock, TokenFault,
};
use hangar_core::{Authenticator, PrincipalId};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

fn params(url: &url::Url) -> HashMap<String, String> {
    url.query_pairs().into_owned().collect()
}

#[tokio::test]
async fn a_good_sign_in_returns_a_session_that_authenticate_accepts() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let signed_in = sign_in(&authenticator, &fake).await.expect("a sign-in");
    let expected = PrincipalId::try_from(PRINCIPAL).unwrap();
    assert_eq!(signed_in.principal().id(), &expected);
    assert_eq!(signed_in.return_to(), &return_to());
    assert_eq!(signed_in.expires_in(), Duration::from_secs(12 * 60 * 60));
    let principal = authenticator
        .authenticate(signed_in.session())
        .await
        .unwrap();
    assert_eq!(principal.id(), &expected);
}

#[tokio::test]
async fn the_authorisation_url_carries_every_parameter_and_two_starts_share_no_value() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let first = authenticator.start(return_to()).unwrap();
    let second = authenticator.start(return_to()).unwrap();
    let one = params(first.redirect());
    let two = params(second.redirect());
    assert_eq!(first.redirect().path(), "/authorize");
    assert_eq!(one["response_type"], "code");
    assert_eq!(one["client_id"], CLIENT_ID);
    assert_eq!(one["redirect_uri"], REDIRECT);
    assert_eq!(one["scope"], "openid email profile");
    assert_eq!(one["code_challenge_method"], "S256");
    assert!(
        !first.kept().expose().contains(&one["state"]),
        "the kept value is sealed, so the state is not readable in it"
    );
    for name in ["state", "nonce", "code_challenge"] {
        assert_eq!(one[name].len(), 43, "{name} is 32 bytes in base64url");
        assert_ne!(one[name], two[name], "{name} repeats");
    }
    assert_ne!(one["state"], one["nonce"]);
}

#[tokio::test]
async fn a_callback_whose_state_differs_is_refused_and_the_sign_in_then_completes() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let start = authenticator.start(return_to()).unwrap();
    let query = fake.approve(start.redirect());
    // Another sign-in's kept value opens, and its state is not the query's.
    let another = authenticator.start(return_to()).unwrap();
    let other = KeptState::from_cookie(another.kept().expose());
    assert_refused(
        authenticator.complete(&query, Some(&other), None).await,
        &SignInRefusal::WrongState,
    );
    let kept = KeptState::from_cookie(start.kept().expose());
    assert!(
        authenticator
            .complete(&query, Some(&kept), None)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_callback_with_no_kept_state_is_refused_and_one_with_it_completes() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let start = authenticator.start(return_to()).unwrap();
    let query = fake.approve(start.redirect());
    assert_refused(
        authenticator.complete(&query, None, None).await,
        &SignInRefusal::NoKeptState,
    );
    assert!(
        authenticator
            .complete(&query, Some(start.kept()), None)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_second_callback_with_the_same_kept_value_is_refused_by_the_provider_as_code_rejected() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let start = authenticator.start(return_to()).unwrap();
    let query = fake.approve(start.redirect());
    assert!(
        authenticator
            .complete(&query, Some(start.kept()), None)
            .await
            .is_ok()
    );
    assert_refused(
        authenticator
            .complete(&query, Some(start.kept()), None)
            .await,
        &SignInRefusal::CodeRejected,
    );
    assert_eq!(fake.token_requests(), 2);
}

#[tokio::test]
async fn a_kept_value_that_does_not_open_is_refused_as_unknown_and_its_own_completes() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let start = authenticator.start(return_to()).unwrap();
    let query = fake.approve(start.redirect());
    let forged = KeptState::from_cookie("another-state-value");
    assert_refused(
        authenticator.complete(&query, Some(&forged), None).await,
        &SignInRefusal::UnknownState,
    );
    assert_eq!(fake.token_requests(), 0);
    assert!(
        authenticator
            .complete(&query, Some(start.kept()), None)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_kept_value_from_another_authenticator_is_refused_as_unknown() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let (other, _other_clock) = connect(&fake).await;
    let start = other.start(return_to()).unwrap();
    let query = fake.approve(start.redirect());
    assert_refused(
        authenticator
            .complete(&query, Some(start.kept()), None)
            .await,
        &SignInRefusal::UnknownState,
    );
    assert!(
        other
            .complete(&query, Some(start.kept()), None)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_code_the_provider_already_redeemed_is_refused_and_a_fresh_code_is_accepted() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let first = authenticator.start(return_to()).unwrap();
    let query = fake.approve(first.redirect());
    assert!(
        authenticator
            .complete(&query, Some(first.kept()), None)
            .await
            .is_ok()
    );
    // A second sign-in whose callback carries the first sign-in's code.
    let second = authenticator.start(return_to()).unwrap();
    let code = query
        .split('&')
        .find(|pair| pair.starts_with("code="))
        .unwrap();
    let state = &params(second.redirect())["state"];
    let replayed = format!("{code}&state={state}");
    assert_refused(
        authenticator
            .complete(&replayed, Some(second.kept()), None)
            .await,
        &SignInRefusal::CodeRejected,
    );
    assert!(sign_in(&authenticator, &fake).await.is_ok());
}

#[tokio::test]
async fn a_token_whose_nonce_differs_is_refused_and_the_right_nonce_is_accepted() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    assert!(sign_in(&authenticator, &fake).await.is_ok());
    fake.edit_claims(|claims| {
        claims.insert("nonce".into(), json!("another-nonce"));
    });
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::TokenRejected(TokenFault::Nonce),
    );
}

#[tokio::test]
async fn a_token_for_another_audience_for_two_audiences_or_for_another_azp_is_refused() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let audience = SignInRefusal::TokenRejected(TokenFault::Audience);
    fake.edit_claims(|claims| {
        claims.insert("aud".into(), json!("another-client"));
    });
    assert_refused(sign_in(&authenticator, &fake).await, &audience);
    fake.edit_claims(|claims| {
        claims.insert("aud".into(), json!([CLIENT_ID, "another-client"]));
    });
    assert_refused(sign_in(&authenticator, &fake).await, &audience);
    fake.edit_claims(|claims| {
        claims.insert("azp".into(), json!("another-client"));
    });
    assert_refused(sign_in(&authenticator, &fake).await, &audience);
    // The admitted cases: the client ID alone, as a string or a list of one.
    fake.edit_claims(|claims| {
        claims.insert("aud".into(), json!([CLIENT_ID]));
        claims.remove("azp");
    });
    assert!(sign_in(&authenticator, &fake).await.is_ok());
}

#[tokio::test]
async fn a_token_from_another_issuer_is_refused_and_the_configured_issuer_is_accepted() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    assert!(sign_in(&authenticator, &fake).await.is_ok());
    fake.edit_claims(|claims| {
        claims.insert("iss".into(), json!("https://accounts.example.net"));
    });
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::TokenRejected(TokenFault::Issuer),
    );
}

#[tokio::test]
async fn an_expired_token_is_refused_and_one_expired_thirty_seconds_ago_is_accepted() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    fake.edit_claims(|claims| {
        claims.insert("exp".into(), json!(now_seconds() - 120));
    });
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::TokenRejected(TokenFault::Expired),
    );
    fake.edit_claims(|claims| {
        claims.insert("exp".into(), json!(now_seconds() - 30));
    });
    assert!(sign_in(&authenticator, &fake).await.is_ok());
}

#[tokio::test]
async fn an_unverified_or_missing_email_is_refused_and_a_verified_one_is_accepted() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    for verified in [json!(false), json!("true")] {
        fake.edit_claims(move |claims| {
            claims.insert("email_verified".into(), verified.clone());
        });
        assert_refused(
            sign_in(&authenticator, &fake).await,
            &SignInRefusal::EmailUnverified,
        );
    }
    fake.edit_claims(|claims| {
        claims.remove("email");
    });
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::EmailUnverified,
    );
    fake.edit_claims(|claims| {
        claims.insert("email_verified".into(), json!(true));
    });
    assert!(sign_in(&authenticator, &fake).await.is_ok());
}

#[tokio::test]
async fn an_email_off_the_allow_list_is_refused_and_one_in_another_letter_case_is_accepted() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    fake.set_email("eve@example.com");
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::NotAllowed,
    );
    fake.set_email(&EMAIL.to_uppercase());
    assert!(sign_in(&authenticator, &fake).await.is_ok());
}

#[tokio::test]
async fn a_token_with_an_unknown_key_id_is_refused_after_one_refresh() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    assert!(sign_in(&authenticator, &fake).await.is_ok());
    let before = fake.key_set_requests();
    fake.new_unpublished_key();
    fake.forge(Forge::UnpublishedKey);
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::TokenRejected(TokenFault::UnknownKey),
    );
    assert_eq!(fake.key_set_requests(), before + 1);
}

#[tokio::test]
async fn a_token_signed_by_a_rotated_key_is_accepted_after_one_refresh() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let before = fake.key_set_requests();
    fake.rotate_key();
    assert!(sign_in(&authenticator, &fake).await.is_ok());
    assert_eq!(fake.key_set_requests(), before + 1);
}

#[tokio::test]
async fn a_second_unknown_key_id_within_sixty_seconds_causes_no_second_fetch() {
    let fake = Fake::start().await;
    let (authenticator, clock) = connect(&fake).await;
    let before = fake.key_set_requests();
    fake.new_unpublished_key();
    fake.forge(Forge::UnpublishedKey);
    let unknown = SignInRefusal::TokenRejected(TokenFault::UnknownKey);
    assert_refused(sign_in(&authenticator, &fake).await, &unknown);
    fake.new_unpublished_key();
    clock.advance(Duration::from_secs(59));
    assert_refused(sign_in(&authenticator, &fake).await, &unknown);
    assert_eq!(fake.key_set_requests(), before + 1);
    // The admitted case: sixty seconds later the crate fetches again.
    clock.advance(Duration::from_secs(1));
    fake.rotate_key();
    fake.forge(Forge::None);
    assert!(sign_in(&authenticator, &fake).await.is_ok());
    assert_eq!(fake.key_set_requests(), before + 2);
}

#[tokio::test]
async fn an_alg_none_or_hs256_token_is_refused_without_a_key_set_fetch() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let before = fake.key_set_requests();
    for forge in [Forge::AlgNone, Forge::Hs256] {
        fake.forge(forge);
        assert_refused(
            sign_in(&authenticator, &fake).await,
            &SignInRefusal::TokenRejected(TokenFault::Algorithm),
        );
    }
    assert_eq!(fake.key_set_requests(), before);
    fake.forge(Forge::None);
    assert!(sign_in(&authenticator, &fake).await.is_ok());
}

#[tokio::test]
async fn a_token_with_a_bad_signature_is_refused_and_a_good_signature_is_accepted() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    fake.forge(Forge::BadSignature);
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::TokenRejected(TokenFault::Signature),
    );
    fake.forge(Forge::None);
    assert!(sign_in(&authenticator, &fake).await.is_ok());
}

#[tokio::test]
async fn a_provider_error_callback_is_refused_without_a_token_request() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let start = authenticator.start(return_to()).unwrap();
    let query = fake.deny(start.redirect());
    assert_refused(
        authenticator
            .complete(&query, Some(start.kept()), None)
            .await,
        &SignInRefusal::ProviderDenied,
    );
    assert_eq!(fake.token_requests(), 0);
    assert!(sign_in(&authenticator, &fake).await.is_ok());
}

#[tokio::test]
async fn a_token_endpoint_that_fails_times_out_rate_limits_or_stalls_gives_provider_unavailable() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    for reply in [
        TokenReply::Status(500),
        TokenReply::Status(503),
        TokenReply::Status(408),
        TokenReply::Status(429),
        TokenReply::Stall,
    ] {
        fake.token_reply(reply);
        let refusal = sign_in(&authenticator, &fake).await.unwrap_err();
        assert_eq!(refusal, SignInRefusal::ProviderUnavailable);
        assert!(refusal.is_temporary());
    }
    fake.token_reply(TokenReply::Normal);
    assert!(sign_in(&authenticator, &fake).await.is_ok());
}

#[tokio::test]
async fn a_token_reply_without_an_id_token_gives_provider_misbehaved() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    fake.token_reply(TokenReply::NoIdToken);
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::ProviderMisbehaved,
    );
    fake.token_reply(TokenReply::Normal);
    assert!(sign_in(&authenticator, &fake).await.is_ok());
}

#[tokio::test]
async fn a_pending_sign_in_older_than_ten_minutes_is_refused_and_a_younger_one_completes() {
    let fake = Fake::start().await;
    let (authenticator, clock) = connect(&fake).await;
    let old = authenticator.start(return_to()).unwrap();
    clock.advance(Duration::from_secs(1));
    let young = authenticator.start(return_to()).unwrap();
    clock.advance(Duration::from_secs(599));
    let old_query = fake.approve(old.redirect());
    assert_refused(
        authenticator
            .complete(&old_query, Some(old.kept()), None)
            .await,
        &SignInRefusal::UnknownState,
    );
    let young_query = fake.approve(young.redirect());
    assert!(
        authenticator
            .complete(&young_query, Some(young.kept()), None)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_sign_in_completes_after_a_thousand_later_starts() {
    let fake = Fake::start().await;
    let (authenticator, clock) = connect(&fake).await;
    let first = authenticator.start(return_to()).unwrap();
    for _ in 0..1000 {
        clock.advance(Duration::from_millis(1));
        authenticator.start(return_to()).unwrap();
    }
    let query = fake.approve(first.redirect());
    assert!(
        authenticator
            .complete(&query, Some(first.kept()), None)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn the_provider_receives_the_verifier_and_the_client_secret_only_in_the_token_body() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let start = authenticator.start(return_to()).unwrap();
    let authorization = start.redirect().to_string();
    let query = fake.approve(start.redirect());
    assert!(
        authenticator
            .complete(&query, Some(start.kept()), None)
            .await
            .is_ok()
    );
    let seen = fake.seen();
    let token = seen.iter().find(|seen| seen.path == "/token").unwrap();
    assert_eq!(token.method, "POST");
    let body: HashMap<String, String> = url::form_urlencoded::parse(token.body.as_bytes())
        .into_owned()
        .collect();
    let verifier = body["code_verifier"].clone();
    assert_eq!(body["client_secret"], CLIENT_SECRET);
    assert!(!authorization.contains(&verifier));
    assert!(!authorization.contains(CLIENT_SECRET));
    for request in &seen {
        for secret in [verifier.as_str(), CLIENT_SECRET] {
            assert!(!request.query.contains(secret), "{}", request.path);
            assert!(!request.headers.contains(secret), "{}", request.path);
            if request.path != "/token" {
                assert!(!request.body.contains(secret), "{}", request.path);
            }
        }
    }
}

#[tokio::test]
async fn an_email_with_a_non_ascii_letter_that_lower_cases_to_an_allowed_one_is_refused() {
    let fake = Fake::start().await;
    let allow = AllowList::new(vec![AllowEntry::new(
        Email::try_from("kate@example.com").unwrap(),
        PrincipalId::try_from("kate").unwrap(),
        Reach::Everything,
    )])
    .unwrap();
    let authenticator = OidcAuthenticator::connect_with_clock(
        config_with(&fake, Admit::Emails(allow)),
        &secret(),
        TestClock::new(),
    )
    .await
    .unwrap();
    // U+212A KELVIN SIGN lower-cases to an ASCII k under Unicode rules.
    fake.set_email("\u{212A}ate@example.com");
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::NotAllowed,
    );
    fake.set_email("KATE@example.com");
    assert!(sign_in(&authenticator, &fake).await.is_ok());
}

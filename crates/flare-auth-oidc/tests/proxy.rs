//! The proxy sign-in: a token that a single sign-on proxy signed is
//! accepted from the cookie or as a bearer only when every check passes,
//! and each check refuses its own fault. Every refusal test has a twin that
//! shows the admitted case.
#![cfg(test)]

#[path = "support/fake_proxy.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the fake issuer is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod fake_proxy;

use fake_proxy::{
    AUDIENCE, DOMAIN, EMAIL, FakeProxy, KeysReply, PRINCIPAL, Sign, allow_list, domain_rule, home,
};
use flare_auth_oidc::{
    Admit, IdentityClaim, ProxyAlgorithm, ProxyAlgorithms, ProxyAuthenticator, ProxyConnectError,
    ProxyRefusal, Reach,
};
use flare_core::{
    Access, AuthError, Authenticator as _, Credential, Decision, PageName, Principal, Resource,
};
use serde_json::{Value, json};
use std::time::Duration;

async fn check(
    authenticator: &ProxyAuthenticator<flare_auth_oidc::TestClock>,
    token: &str,
) -> Result<Principal, ProxyRefusal> {
    authenticator.check(&Credential::session(token)).await
}

#[tokio::test]
async fn an_rs256_token_in_the_cookie_signs_the_allow_listed_person_in() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let principal = check(&proxy, &fake.token(Sign::Rs256)).await.unwrap();
    assert_eq!(principal.id().as_str(), PRINCIPAL);
    assert_eq!(principal.email().unwrap().as_str(), EMAIL);
}

#[tokio::test]
async fn an_es256_token_signs_the_person_in() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let principal = check(&proxy, &fake.token(Sign::Es256)).await.unwrap();
    assert_eq!(principal.id().as_str(), PRINCIPAL);
}

#[tokio::test]
async fn the_same_token_as_a_bearer_signs_the_person_in() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let bearer = Credential::bearer(fake.token(Sign::Rs256));
    let principal = proxy.authenticate(&bearer).await.unwrap();
    assert_eq!(principal.id().as_str(), PRINCIPAL);
}

#[tokio::test]
async fn an_unsigned_token_is_refused_before_any_key_is_fetched() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let before = fake.fetches();
    let refused = check(&proxy, &fake.token(Sign::AlgNone)).await;
    assert_eq!(refused, Err(ProxyRefusal::Algorithm));
    assert_eq!(fake.fetches(), before);
}

#[tokio::test]
async fn an_hs256_token_keyed_with_the_public_key_is_refused() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let refused = check(&proxy, &fake.token(Sign::Hs256WithPublicKey)).await;
    assert_eq!(refused, Err(ProxyRefusal::Algorithm));
}

#[tokio::test]
async fn an_algorithm_the_config_does_not_list_is_refused_and_a_listed_one_accepted() {
    let fake = FakeProxy::start().await;
    let rs256_only = ProxyAlgorithms::new(&[ProxyAlgorithm::Rs256]).unwrap();
    let config = fake
        .config(Admit::Emails(allow_list()))
        .with_algorithms(rs256_only);
    let (proxy, _clock) = fake.connect_config(config).await;
    let refused = check(&proxy, &fake.token(Sign::Es256)).await;
    assert_eq!(refused, Err(ProxyRefusal::Algorithm));
    assert!(check(&proxy, &fake.token(Sign::Rs256)).await.is_ok());
}

#[tokio::test]
async fn a_changed_signature_is_refused() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let refused = check(&proxy, &fake.token(Sign::BadSignature)).await;
    assert_eq!(refused, Err(ProxyRefusal::Signature));
}

#[tokio::test]
async fn an_unknown_key_is_refused_after_one_refetch_and_a_second_waits_a_minute() {
    let fake = FakeProxy::start().await;
    let (proxy, clock) = fake.connect().await;
    let before = fake.fetches();
    let refused = check(&proxy, &fake.token(Sign::UnpublishedKey)).await;
    assert_eq!(refused, Err(ProxyRefusal::UnknownKey));
    assert_eq!(fake.fetches(), before + 1);
    let again = check(&proxy, &fake.token(Sign::UnpublishedKey)).await;
    assert_eq!(again, Err(ProxyRefusal::UnknownKey));
    assert_eq!(
        fake.fetches(),
        before + 1,
        "no second fetch within a minute"
    );
    clock.advance(Duration::from_secs(61));
    let _ = check(&proxy, &fake.token(Sign::UnpublishedKey)).await;
    assert_eq!(fake.fetches(), before + 2);
}

#[tokio::test]
async fn a_key_the_key_set_no_longer_lists_is_refused_after_a_refetch() {
    let fake = FakeProxy::start().await;
    let (proxy, clock) = fake.connect().await;
    assert!(check(&proxy, &fake.token(Sign::Es256)).await.is_ok());
    fake.unpublish(fake_proxy::EC_KID);
    // The cached set is used for an hour, so the key is still known.
    assert!(check(&proxy, &fake.token(Sign::Es256)).await.is_ok());
    clock.advance(Duration::from_secs(60 * 60 + 1));
    let refused = check(&proxy, &fake.token(Sign::Es256)).await;
    assert_eq!(refused, Err(ProxyRefusal::UnknownKey));
}

#[tokio::test]
async fn an_unknown_key_whose_refetch_fails_leaves_the_proxy_unavailable() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    fake.keys_reply(KeysReply::Down);
    let answer = proxy
        .authenticate(&Credential::session(fake.token(Sign::UnpublishedKey)))
        .await;
    assert_eq!(answer, Err(AuthError::Unavailable));
    // The failed fetch counts against the one-a-minute limit, so the next
    // unknown key fetches nothing and is still unavailable, because the
    // last fetch failed.
    let before = fake.fetches();
    let refused = check(&proxy, &fake.token(Sign::UnpublishedKey)).await;
    assert_eq!(refused, Err(ProxyRefusal::KeysUnavailable));
    assert_eq!(fake.fetches(), before);
}

/// A token that every check accepts once `clock` has moved on by `ahead`
/// seconds from now.
fn token_ahead(fake: &FakeProxy, sign: Sign, ahead: u64) -> String {
    let now = fake_proxy::now_seconds() + ahead;
    fake.token_with(sign, |claims| {
        claims.insert("iat".into(), serde_json::json!(now));
        claims.insert("exp".into(), serde_json::json!(now + 300));
    })
}

#[tokio::test]
async fn a_key_set_over_an_hour_old_keeps_its_keys_while_its_address_is_down_and_fetches_once_a_minute()
 {
    let fake = FakeProxy::start().await;
    let (proxy, clock) = fake.connect().await;
    fake.keys_reply(KeysReply::Down);
    let hour = 60 * 60 + 1;
    clock.advance(Duration::from_secs(hour));
    let before = fake.fetches();
    for _ in 0..3 {
        let token = token_ahead(&fake, Sign::Rs256, hour);
        assert!(
            check(&proxy, &token).await.is_ok(),
            "the held key still verifies"
        );
    }
    assert_eq!(fake.fetches(), before + 1, "one fetch for three requests");
    clock.advance(Duration::from_secs(61));
    let token = token_ahead(&fake, Sign::Rs256, hour + 61);
    assert!(check(&proxy, &token).await.is_ok());
    assert_eq!(
        fake.fetches(),
        before + 2,
        "a minute later it fetches again"
    );
    // The admitted twin: once the address answers, the next fetch replaces
    // the set, and an unpublished key is refused.
    fake.keys_reply(KeysReply::Normal);
    fake.unpublish(fake_proxy::RSA_KID);
    clock.advance(Duration::from_secs(61));
    let token = token_ahead(&fake, Sign::Rs256, hour + 122);
    assert_eq!(check(&proxy, &token).await, Err(ProxyRefusal::UnknownKey));
}

#[tokio::test]
async fn a_header_algorithm_of_another_family_than_its_key_is_refused_and_a_matching_one_accepted()
{
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    for sign in [Sign::Es256NamingTheRsaKey, Sign::Rs256NamingTheEcKey] {
        let refused = check(&proxy, &fake.token(sign)).await;
        assert_eq!(refused, Err(ProxyRefusal::Algorithm), "{sign:?}");
    }
    for sign in [Sign::Es256, Sign::Rs256] {
        assert!(check(&proxy, &fake.token(sign)).await.is_ok(), "{sign:?}");
    }
}

#[tokio::test]
async fn the_refusal_of_an_unavailable_key_set_is_temporary() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    fake.keys_reply(KeysReply::Down);
    let refused = check(&proxy, &fake.token(Sign::UnpublishedKey)).await;
    assert_eq!(refused, Err(ProxyRefusal::KeysUnavailable));
    assert!(ProxyRefusal::KeysUnavailable.is_temporary());
    assert!(!ProxyRefusal::Signature.is_temporary());
}

#[tokio::test]
async fn a_token_for_another_application_is_refused_and_an_audience_list_holding_ours_accepted() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let other = fake.token_with(Sign::Rs256, |claims| {
        claims.insert("aud".into(), json!("another-app"));
    });
    assert_eq!(check(&proxy, &other).await, Err(ProxyRefusal::Audience));
    let listed = fake.token_with(Sign::Rs256, |claims| {
        claims.insert("aud".into(), json!(["another-app", AUDIENCE]));
    });
    assert!(check(&proxy, &listed).await.is_ok());
}

#[tokio::test]
async fn a_token_from_another_issuer_is_refused() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let other = fake.token_with(Sign::Rs256, |claims| {
        claims.insert("iss".into(), json!("https://sso.example.com/other"));
    });
    assert_eq!(check(&proxy, &other).await, Err(ProxyRefusal::Issuer));
}

fn at(offset: i64) -> Value {
    let now = i64::try_from(fake_proxy::now_seconds()).unwrap();
    json!(now + offset)
}

#[tokio::test]
async fn a_token_expired_past_the_leeway_is_refused_and_one_within_it_accepted() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let expired = fake.token_with(Sign::Rs256, |claims| {
        claims.insert("exp".into(), at(-120));
    });
    assert_eq!(check(&proxy, &expired).await, Err(ProxyRefusal::Expired));
    let within = fake.token_with(Sign::Rs256, |claims| {
        claims.insert("exp".into(), at(-30));
    });
    assert!(check(&proxy, &within).await.is_ok());
}

#[tokio::test]
async fn a_token_with_no_expiry_is_refused() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let endless = fake.token_with(Sign::Rs256, |claims| {
        claims.remove("exp");
    });
    assert_eq!(check(&proxy, &endless).await, Err(ProxyRefusal::Malformed));
}

#[tokio::test]
async fn a_token_not_yet_valid_or_issued_ahead_is_refused_and_one_within_the_leeway_accepted() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    for claim in ["nbf", "iat"] {
        let ahead = fake.token_with(Sign::Rs256, |claims| {
            claims.insert(claim.into(), at(120));
        });
        assert_eq!(
            check(&proxy, &ahead).await,
            Err(ProxyRefusal::NotYetValid),
            "{claim}"
        );
        let within = fake.token_with(Sign::Rs256, |claims| {
            claims.insert(claim.into(), at(30));
        });
        assert!(check(&proxy, &within).await.is_ok(), "{claim}");
    }
}

#[tokio::test]
async fn a_token_without_an_address_in_the_identity_claim_is_refused() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let missing = fake.token_with(Sign::Rs256, |claims| {
        claims.remove("email");
    });
    assert_eq!(check(&proxy, &missing).await, Err(ProxyRefusal::NoIdentity));
    let not_an_address = fake.token_with(Sign::Rs256, |claims| {
        claims.insert("email".into(), json!("ada"));
    });
    assert_eq!(
        check(&proxy, &not_an_address).await,
        Err(ProxyRefusal::NoIdentity)
    );
}

#[tokio::test]
async fn a_configured_identity_claim_names_the_person() {
    let fake = FakeProxy::start().await;
    let config = fake
        .config(Admit::Emails(allow_list()))
        .with_identity_claim(IdentityClaim::try_from("upn").unwrap());
    let (proxy, _clock) = fake.connect_config(config).await;
    let token = fake.token_with(Sign::Rs256, |claims| {
        claims.remove("email");
        claims.insert("upn".into(), json!(EMAIL));
    });
    assert_eq!(
        check(&proxy, &token).await.unwrap().id().as_str(),
        PRINCIPAL
    );
}

#[tokio::test]
async fn an_unverified_address_is_refused_and_one_with_no_verified_claim_accepted() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let unverified = fake.token_with(Sign::Rs256, |claims| {
        claims.insert("email_verified".into(), json!(false));
    });
    assert_eq!(
        check(&proxy, &unverified).await,
        Err(ProxyRefusal::EmailUnverified)
    );
    let unstated = fake.token_with(Sign::Rs256, |claims| {
        claims.remove("email_verified");
    });
    assert!(check(&proxy, &unstated).await.is_ok());
}

#[tokio::test]
async fn an_address_off_the_allow_list_is_not_admitted() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let stranger = fake.token_with(Sign::Rs256, |claims| {
        claims.insert("email".into(), json!("eve@example.com"));
    });
    assert_eq!(
        check(&proxy, &stranger).await,
        Err(ProxyRefusal::NotAdmitted)
    );
}

#[tokio::test]
async fn the_domain_rule_compares_the_address_domain_exactly() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect_config(fake.config(domain_rule())).await;
    let colleague = fake.token_with(Sign::Rs256, |claims| {
        claims.insert(
            "email".into(),
            json!(format!("Grace@{}", DOMAIN.to_uppercase())),
        );
    });
    let principal = check(&proxy, &colleague).await.unwrap();
    assert_eq!(principal.id().as_str(), "grace@example.com");
    for outsider in [
        "eve@other.com",
        "eve@sub.example.com",
        "eve@example.com.evil",
    ] {
        let token = fake.token_with(Sign::Rs256, |claims| {
            claims.insert("email".into(), json!(outsider));
        });
        assert_eq!(
            check(&proxy, &token).await,
            Err(ProxyRefusal::NotAdmitted),
            "{outsider}"
        );
    }
}

#[tokio::test]
async fn a_token_over_8_kib_is_refused_before_it_is_decoded() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let big = fake.token_with(Sign::Rs256, |claims| {
        claims.insert("padding".into(), json!("x".repeat(8 * 1024)));
    });
    assert_eq!(check(&proxy, &big).await, Err(ProxyRefusal::TooLong));
    let fits = fake.token_with(Sign::Rs256, |claims| {
        claims.insert("padding".into(), json!("x".repeat(4 * 1024)));
    });
    assert!(check(&proxy, &fits).await.is_ok());
}

#[tokio::test]
async fn authorize_follows_the_admit_rule_reach() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect_config(fake.config(domain_rule())).await;
    let principal = check(&proxy, &fake.token(Sign::Rs256)).await.unwrap();
    let elsewhere = Resource::Page(PageName::try_from("elsewhere").unwrap());
    let home_answer = proxy.authorize(&principal, Access::Read, &home()).await;
    let elsewhere_answer = proxy.authorize(&principal, Access::Read, &elsewhere).await;
    assert_eq!(home_answer, Ok(Decision::Allowed));
    assert_eq!(elsewhere_answer, Ok(Decision::Denied));
    let (anyone, _clock) = fake
        .connect_config(fake.config(Admit::Anyone(Reach::Everything)))
        .await;
    let answer = anyone
        .authorize(&principal, Access::Write, &elsewhere)
        .await;
    assert_eq!(answer, Ok(Decision::Allowed));
}

#[tokio::test]
async fn connect_fails_when_the_key_set_cannot_be_fetched_and_succeeds_when_it_can() {
    let fake = FakeProxy::start().await;
    fake.keys_reply(KeysReply::Down);
    let config = fake.config(Admit::Emails(allow_list()));
    let refused = ProxyAuthenticator::connect(config.clone()).await;
    assert_eq!(refused.err(), Some(ProxyConnectError::Unreachable));
    fake.keys_reply(KeysReply::Normal);
    assert!(ProxyAuthenticator::connect(config).await.is_ok());
}

#[tokio::test]
async fn connect_fails_on_a_key_set_with_no_usable_key_or_more_than_20_keys() {
    let fake = FakeProxy::start().await;
    let config = fake.config(Admit::Emails(allow_list()));
    let symmetric = json!({ "keys": [{ "kty": "oct", "kid": "k", "k": "c2VjcmV0" }] });
    fake.keys_reply(KeysReply::Json(symmetric));
    let refused = ProxyAuthenticator::connect(config.clone()).await;
    assert_eq!(refused.err(), Some(ProxyConnectError::Unusable));
    let many: Vec<Value> = (0..21)
        .map(|n| json!({ "kty": "EC", "crv": "P-256", "kid": format!("k{n}"), "x": "AA", "y": "AA" }))
        .collect();
    fake.keys_reply(KeysReply::Json(json!({ "keys": many })));
    let refused = ProxyAuthenticator::connect(config).await;
    assert_eq!(refused.err(), Some(ProxyConnectError::Unusable));
}

#[tokio::test]
async fn a_refusal_names_its_check_and_never_the_token() {
    let fake = FakeProxy::start().await;
    let (proxy, _clock) = fake.connect().await;
    let token = fake.token(Sign::BadSignature);
    let refusal = check(&proxy, &token).await.unwrap_err();
    assert_eq!(refusal.check(), "signature");
    let shown = format!("{refusal} {refusal:?} {proxy:?}");
    assert!(!shown.contains(&token), "{shown}");
    assert!(!shown.contains(EMAIL), "{shown}");
}

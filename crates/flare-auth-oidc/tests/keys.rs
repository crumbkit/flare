//! The provider's key set: the checks at `connect`, a refresh that returns
//! an unusable set, the one hour limit on the cached set, and sign-ins that
//! wait for one fetch.
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
    DOMAIN, Fake, Forge, KeysReply, assert_refused, config, connect, connect_domain, domain_config,
    one_key_set, secret, sign_in,
};
use flare_auth_oidc::{ConnectError, OidcAuthenticator, SignInRefusal};
use serde_json::json;
use std::time::Duration;

fn unusable_key_sets() -> [KeysReply; 4] {
    [
        KeysReply::Empty,
        KeysReply::Json(one_key_set(|key| {
            key.insert("alg".into(), json!("RS512"));
        })),
        KeysReply::Json(one_key_set(|key| {
            key.insert("use".into(), json!("enc"));
        })),
        KeysReply::Json(
            json!({ "keys": [{ "kty": "EC", "crv": "P-256", "kid": "ec", "x": "AA", "y": "AA" }] }),
        ),
    ]
}

async fn connect_with_keys(reply: KeysReply) -> Result<(), ConnectError> {
    let fake = Fake::start().await;
    fake.keys_reply(reply);
    OidcAuthenticator::connect(config(&fake), &secret())
        .await
        .map(|_| ())
}

#[tokio::test]
async fn an_empty_or_unusable_key_set_at_connect_fails_and_one_rsa_signing_key_is_enough() {
    let ec_only =
        json!({ "keys": [{ "kty": "EC", "crv": "P-256", "kid": "ec", "x": "AA", "y": "AA" }] });
    let encryption_only = one_key_set(|key| {
        key.insert("use".into(), json!("enc"));
    });
    let other_alg = one_key_set(|key| {
        key.insert("alg".into(), json!("RS512"));
    });
    for reply in [
        KeysReply::Empty,
        KeysReply::Json(ec_only),
        KeysReply::Json(encryption_only),
        KeysReply::Json(other_alg),
        KeysReply::Down,
    ] {
        assert_eq!(connect_with_keys(reply).await, Err(ConnectError::Keys));
    }
    let bare = one_key_set(|key| {
        key.remove("use");
        key.remove("alg");
    });
    assert_eq!(connect_with_keys(KeysReply::Json(bare)).await, Ok(()));
}

#[tokio::test]
async fn a_refresh_that_returns_an_unusable_key_set_leaves_the_cached_keys_in_place() {
    let fake = Fake::start().await;
    let (authenticator, clock) = connect(&fake).await;
    let unusable = [
        KeysReply::Empty,
        KeysReply::Json(one_key_set(|key| {
            key.insert("alg".into(), json!("RS512"));
        })),
        KeysReply::Json(one_key_set(|key| {
            key.insert("use".into(), json!("enc"));
        })),
        KeysReply::Json(
            json!({ "keys": [{ "kty": "EC", "crv": "P-256", "kid": "ec", "x": "AA", "y": "AA" }] }),
        ),
    ];
    for reply in unusable {
        fake.keys_reply(reply);
        fake.new_unpublished_key();
        fake.forge(Forge::UnpublishedKey);
        assert_refused(
            sign_in(&authenticator, &fake).await,
            &SignInRefusal::ProviderMisbehaved,
        );
        // The admitted case: the key the cache held before still verifies.
        fake.forge(Forge::None);
        assert!(sign_in(&authenticator, &fake).await.is_ok());
        clock.advance(Duration::from_secs(60));
    }
}

/// Waits until the fake has received both sign-ins' token requests and
/// `keys` key set requests.
async fn until_requests(fake: &Fake, keys: usize) {
    while fake.token_requests() < 2 || fake.key_set_requests() < keys {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn two_sign_ins_that_present_a_rotated_key_together_both_succeed_with_one_fetch() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let before = fake.key_set_requests();
    fake.rotate_key();
    // The first sign-in's fetch is held until the fake has answered both
    // token requests, so the second sign-in reaches the key cache while the
    // fetch is in flight. The unit tests in src/keys/tests.rs fix that order
    // by polling each sign-in by hand; this test checks the same outcome
    // through `complete`.
    fake.keys_reply(KeysReply::Hold);
    let release = async {
        until_requests(&fake, before + 1).await;
        fake.keys_reply(KeysReply::Normal);
    };
    let (first, second, ()) = tokio::join!(
        sign_in(&authenticator, &fake),
        sign_in(&authenticator, &fake),
        release
    );
    assert!(first.is_ok(), "{first:?}");
    assert!(second.is_ok(), "{second:?}");
    assert_eq!(fake.key_set_requests(), before + 1);
}

#[tokio::test]
async fn two_sign_ins_that_wait_for_one_failed_fetch_both_get_provider_unavailable() {
    let fake = Fake::start().await;
    let (authenticator, clock) = connect(&fake).await;
    let before = fake.key_set_requests();
    clock.advance(Duration::from_secs(60 * 60));
    // The held fetch is never released, so it fails when the two second
    // request deadline passes. The second sign-in has those two seconds to
    // reach the key cache and wait for the fetch.
    fake.keys_reply(KeysReply::Hold);
    let (first, second) = tokio::join!(
        sign_in(&authenticator, &fake),
        sign_in(&authenticator, &fake)
    );
    assert_refused(first, &SignInRefusal::ProviderUnavailable);
    assert_refused(second, &SignInRefusal::ProviderUnavailable);
    assert_eq!(fake.key_set_requests(), before + 1);
}

#[tokio::test]
async fn a_key_set_older_than_one_hour_is_fetched_again_before_use() {
    let fake = Fake::start().await;
    let (authenticator, clock) = connect(&fake).await;
    let before = fake.key_set_requests();
    clock.advance(Duration::from_secs(59 * 60));
    assert!(sign_in(&authenticator, &fake).await.is_ok());
    assert_eq!(fake.key_set_requests(), before);
    clock.advance(Duration::from_secs(60));
    assert!(sign_in(&authenticator, &fake).await.is_ok());
    assert_eq!(fake.key_set_requests(), before + 1);
}

#[tokio::test]
async fn a_failed_fetch_of_a_stale_key_set_gives_provider_unavailable() {
    let fake = Fake::start().await;
    let (authenticator, clock) = connect(&fake).await;
    clock.advance(Duration::from_secs(60 * 60));
    fake.keys_reply(KeysReply::Down);
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::ProviderUnavailable,
    );
    fake.keys_reply(KeysReply::Normal);
    assert!(sign_in(&authenticator, &fake).await.is_ok());
}

#[tokio::test]
async fn under_the_domain_rule_an_unusable_key_set_fails_connect_and_is_never_cached() {
    for reply in unusable_key_sets() {
        let fake = Fake::start().await;
        fake.keys_reply(reply);
        assert_eq!(
            OidcAuthenticator::connect(domain_config(&fake), &secret())
                .await
                .map(|_| ()),
            Err(ConnectError::Keys)
        );
    }
    let fake = Fake::start().await;
    fake.set_hd(Some(json!(DOMAIN)));
    let (authenticator, clock) = connect_domain(&fake).await;
    for reply in unusable_key_sets() {
        fake.keys_reply(reply);
        fake.new_unpublished_key();
        fake.forge(Forge::UnpublishedKey);
        assert_refused(
            sign_in(&authenticator, &fake).await,
            &SignInRefusal::ProviderMisbehaved,
        );
        // The admitted case: a token signed by the key cached before, with
        // the matching hd, is still accepted.
        fake.forge(Forge::None);
        assert!(sign_in(&authenticator, &fake).await.is_ok());
        clock.advance(Duration::from_secs(60));
    }
}

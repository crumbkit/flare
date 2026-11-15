//! The provider's key set: the checks at `connect`, a refresh that returns
//! an unusable set, and the one hour limit on the cached set.
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
    Fake, Forge, KeysReply, assert_refused, config, connect, one_key_set, secret, sign_in,
};
use hangar_auth_oidc::{ConnectError, OidcAuthenticator, SignInRefusal};
use serde_json::json;
use std::time::Duration;

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
async fn a_refresh_that_returns_an_empty_key_set_leaves_the_cached_keys_in_place() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    fake.keys_reply(KeysReply::Empty);
    fake.new_unpublished_key();
    fake.forge(Forge::UnpublishedKey);
    assert_refused(
        sign_in(&authenticator, &fake).await,
        &SignInRefusal::ProviderMisbehaved,
    );
    fake.forge(Forge::None);
    assert!(sign_in(&authenticator, &fake).await.is_ok());
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

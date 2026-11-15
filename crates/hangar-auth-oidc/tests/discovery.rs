//! `connect` against good and faulty discovery documents.
#![cfg(test)]

#[path = "support/fake_provider.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    reason = "the fake provider is test support: it panics on a broken fixture, and each test reads what it returns"
)]
pub mod fake_provider;

use fake_provider::{CLIENT_ID, Fake, REDIRECT, allow_list, config, secret};
use hangar_auth_oidc::{
    ClientId, ConfigError, ConnectError, IssuerUrl, MetadataFault, OidcAuthenticator, OidcConfig,
    RedirectUri, SecretError,
};
use serde_json::{Map, Value, json};

/// A change to the discovery document.
type Edit = fn(&mut Map<String, Value>);

async fn connect_after(edit: impl FnOnce(&mut Map<String, Value>)) -> Result<(), ConnectError> {
    let fake = Fake::start().await;
    fake.edit_metadata(edit);
    OidcAuthenticator::connect(config(&fake), &secret())
        .await
        .map(|_| ())
}

#[tokio::test]
async fn connect_succeeds_against_a_correct_provider() {
    assert_eq!(connect_after(|_| {}).await, Ok(()));
}

#[tokio::test]
async fn metadata_without_a_token_endpoint_is_a_permanent_metadata_fault() {
    let result = connect_after(|metadata| {
        metadata.remove("token_endpoint");
    })
    .await;
    assert_eq!(
        result,
        Err(ConnectError::Metadata(MetadataFault::NoTokenEndpoint))
    );
    assert_ne!(result, Err(ConnectError::Unreachable));
}

#[tokio::test]
async fn each_other_metadata_fault_fails_connect_with_its_own_fault() {
    let cases: [(Edit, MetadataFault); 7] = [
        (
            |m| {
                m.remove("authorization_endpoint");
            },
            MetadataFault::NoAuthorizationEndpoint,
        ),
        (
            |m| {
                m.remove("jwks_uri");
            },
            MetadataFault::NoKeySetUri,
        ),
        (
            |m| {
                m.insert("issuer".into(), json!("https://accounts.example.net"));
            },
            MetadataFault::IssuerMismatch,
        ),
        (
            |m| {
                m.insert(
                    "id_token_signing_alg_values_supported".into(),
                    json!(["ES256"]),
                );
            },
            MetadataFault::NoRs256,
        ),
        (
            |m| {
                m.insert("code_challenge_methods_supported".into(), json!(["plain"]));
            },
            MetadataFault::NoS256,
        ),
        (
            |m| {
                m.remove("code_challenge_methods_supported");
            },
            MetadataFault::NoS256,
        ),
        (
            |m| {
                m.insert(
                    "token_endpoint".into(),
                    json!("http://tokens.example.net/token"),
                );
            },
            MetadataFault::InsecureEndpoint,
        ),
    ];
    for (edit, fault) in cases {
        assert_eq!(
            connect_after(edit).await,
            Err(ConnectError::Metadata(fault))
        );
    }
}

#[tokio::test]
async fn a_discovery_endpoint_that_is_down_fails_connect_as_unreachable() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let fake = Fake::start().await;
    let down = OidcConfig::new(
        IssuerUrl::loopback_http(&issuer).unwrap(),
        ClientId::try_from(CLIENT_ID).unwrap(),
        RedirectUri::try_from(REDIRECT).unwrap(),
        allow_list(),
    );
    assert_eq!(
        OidcAuthenticator::connect(down, &secret())
            .await
            .map(|_| ()),
        Err(ConnectError::Unreachable)
    );
    assert!(
        OidcAuthenticator::connect(config(&fake), &secret())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_secret_that_cannot_be_read_fails_connect_before_any_request() {
    struct NoSecret;
    impl hangar_auth_oidc::ClientSecretSource for NoSecret {
        fn client_secret(&self) -> Result<secrecy::SecretString, SecretError> {
            Err(SecretError::Unavailable)
        }
    }
    let fake = Fake::start().await;
    assert_eq!(
        OidcAuthenticator::connect(config(&fake), &NoSecret)
            .await
            .map(|_| ()),
        Err(ConnectError::Secret(SecretError::Unavailable))
    );
    assert!(fake.seen().is_empty());
}

#[test]
fn an_http_issuer_is_refused_without_the_test_switch_and_an_https_issuer_is_admitted() {
    assert_eq!(
        IssuerUrl::try_from("http://127.0.0.1:8080"),
        Err(ConfigError::Issuer)
    );
    assert!(IssuerUrl::try_from("https://accounts.google.com").is_ok());
}

#[test]
fn the_test_switch_admits_http_only_on_a_loopback_address() {
    assert!(IssuerUrl::loopback_http("http://127.0.0.1:8080").is_ok());
    assert_eq!(
        IssuerUrl::loopback_http("http://accounts.example.net"),
        Err(ConfigError::Issuer)
    );
}

#[tokio::test]
async fn a_discovery_reply_over_64_kib_fails_connect_as_unreachable() {
    let padding = "x".repeat(65 * 1024);
    assert_eq!(
        connect_after(move |metadata| {
            metadata.insert("padding".into(), json!(padding));
        })
        .await,
        Err(ConnectError::Unreachable)
    );
    let padding = "x".repeat(60 * 1024);
    assert_eq!(
        connect_after(move |metadata| {
            metadata.insert("padding".into(), json!(padding));
        })
        .await,
        Ok(())
    );
}

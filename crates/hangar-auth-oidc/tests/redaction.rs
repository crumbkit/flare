//! The `Debug` and `Display` output of every public value carries no secret,
//! code, token or email.
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
    CLIENT_SECRET, DOMAIN, EMAIL, Fake, config, connect, connect_domain, domain_config, return_to,
    secret, sign_in,
};
use hangar_auth_oidc::{
    ConnectError, Email, FixedSecret, KeptState, MetadataFault, SecretError, SignInRefusal,
    TokenFault,
};
use secrecy::ExposeSecret;
use serde_json::json;

fn assert_hidden(shown: &str, secrets: &[&str]) {
    for secret in secrets {
        assert!(!shown.contains(secret), "{secret:?} appears in {shown}");
    }
}

#[tokio::test]
async fn debug_output_of_the_config_authenticator_start_and_sign_in_hides_every_secret() {
    let fake = Fake::start().await;
    let (authenticator, _clock) = connect(&fake).await;
    let start = authenticator.start(return_to()).unwrap();
    let state = start.kept().expose().to_owned();
    let query = fake.approve(start.redirect());
    let code = query
        .split('&')
        .next()
        .unwrap()
        .trim_start_matches("code=")
        .to_owned();
    let nonce = start
        .redirect()
        .query_pairs()
        .find(|(name, _)| name == "nonce")
        .unwrap()
        .1
        .into_owned();
    let signed_in = authenticator
        .complete(&query, Some(start.kept()))
        .await
        .unwrap();
    let hangar_core::Credential::Session(session) = signed_in.session() else {
        panic!("a session credential");
    };
    let secrets = [
        CLIENT_SECRET,
        EMAIL,
        state.as_str(),
        code.as_str(),
        nonce.as_str(),
        session.expose_secret(),
    ];
    for shown in [
        format!("{:?}", config(&fake)),
        format!("{authenticator:?}"),
        format!("{start:?}"),
        format!("{:?}", start.kept()),
        format!("{:?}", KeptState::from_cookie(&state)),
        format!("{signed_in:?}"),
        format!("{:?}", secret()),
        format!("{:?}", Email::try_from(EMAIL).unwrap()),
        format!("{:?}", FixedSecret::new(CLIENT_SECRET.into())),
    ] {
        assert_hidden(&shown, &secrets);
    }
}

#[test]
fn every_refusal_formats_without_a_secret_or_an_email() {
    let refusals = [
        SignInRefusal::Malformed,
        SignInRefusal::NoKeptState,
        SignInRefusal::WrongState,
        SignInRefusal::UnknownState,
        SignInRefusal::ProviderDenied,
        SignInRefusal::CodeRejected,
        SignInRefusal::ProviderUnavailable,
        SignInRefusal::ProviderMisbehaved,
        SignInRefusal::TokenRejected(TokenFault::Nonce),
        SignInRefusal::EmailUnverified,
        SignInRefusal::NotAllowed,
        SignInRefusal::TooManySessions,
        SignInRefusal::NoRandomness,
        SignInRefusal::NoHostedDomain,
        SignInRefusal::OtherHostedDomain,
    ];
    let errors = [
        ConnectError::Secret(SecretError::Failed),
        ConnectError::Unreachable,
        ConnectError::Metadata(MetadataFault::NoTokenEndpoint),
        ConnectError::Keys,
    ];
    let mut shown: Vec<String> = refusals
        .iter()
        .flat_map(|refusal| [format!("{refusal:?}"), refusal.to_string()])
        .collect();
    shown.extend(
        errors
            .iter()
            .flat_map(|error| [format!("{error:?}"), error.to_string()]),
    );
    for text in shown {
        assert!(!text.is_empty());
        assert_hidden(&text, &[CLIENT_SECRET, EMAIL, "@"]);
    }
}

#[test]
fn the_two_hosted_domain_refusals_format_without_an_email_or_an_hd_value() {
    for refusal in [
        SignInRefusal::NoHostedDomain,
        SignInRefusal::OtherHostedDomain,
    ] {
        assert!(!refusal.is_temporary());
        for text in [format!("{refusal:?}"), refusal.to_string()] {
            assert!(!text.is_empty());
            assert_hidden(&text, &[EMAIL, DOMAIN, "@", "example"]);
        }
    }
}

#[tokio::test]
async fn debug_output_of_a_domain_config_and_a_domain_sign_in_hides_the_email_and_the_domain() {
    let fake = Fake::start().await;
    fake.set_hd(Some(json!(DOMAIN)));
    let (authenticator, _clock) = connect_domain(&fake).await;
    let signed_in = sign_in(&authenticator, &fake).await.unwrap();
    // The principal itself names the email: the admitted value is there.
    assert_eq!(signed_in.principal().id().as_str(), EMAIL);
    for shown in [
        format!("{:?}", domain_config(&fake)),
        format!("{authenticator:?}"),
        format!("{signed_in:?}"),
    ] {
        assert_hidden(&shown, &[EMAIL, DOMAIN]);
    }
}

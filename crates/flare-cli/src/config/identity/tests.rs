//! `hosted.identity` and `hosted.everyone_signed_in`: how a hosted site
//! names people, and whether an everyone grant under an `anyone` rule opens
//! to every signed-in person. Each refusal has a twin that shows the
//! admitted case.

use super::super::{Hosted, Problem, SignIn, Site, parse};
use flare_auth_oidc::Identity;
use flare_core::contract::block_on;
use flare_core::{IdentityKind, Person, RecipientError, Recipients as _};

const HOSTED: &str = r#"{"hosted":{"origin":"https://flare.example.com","listen":"0.0.0.0:8080"},"oidc":{"issuer":"https://accounts.google.com","client_id":"abc","client_secret_file":"/run/secrets/google","admit":{"anyone":true,"reach":"everything"}}}"#;

const PROXY: &str = r#"{"hosted":{"origin":"https://flare.example.com","listen":"0.0.0.0:8080"},"proxy":{"issuer":"https://sso.example.com","audience":"flare","jwks_url":"https://sso.example.com/.well-known/jwks.json","cookie":"proxy_token","algorithms":["RS256"],"admit":{"emails":[{"email":"ana","principal":"ana","reach":"everything"}]}}}"#;

const STORES: &str =
    r#""stores":{"region":"eu-west-1","table":"flare-shares","bucket":"flare-files"}"#;

const USERNAMES: &str = r#""identity":{"username_claim":"preferred_username"}"#;

/// `config` with `extra` added to its hosted section.
fn with(config: &str, extra: &str) -> String {
    config.replacen("\"origin\"", &format!("{extra},\"origin\""), 1)
}

fn hosted(text: &str) -> Hosted {
    match parse(text) {
        Ok(config) => match config.site {
            Site::Hosted(hosted) => hosted,
            Site::Local { .. } => panic!("a local site"),
        },
        Err(Problem::Invalid(problem)) => panic!("refused: {problem}"),
    }
}

fn refusal(text: &str) -> String {
    match parse(text) {
        Err(Problem::Invalid(message)) => message,
        Ok(_) => panic!("accepted: {text}"),
    }
}

fn identity(hosted: &Hosted) -> &Identity {
    match &hosted.sign_in {
        SignIn::Oidc { config, .. } => config.identity(),
        SignIn::Proxy(config) => config.identity(),
    }
}

#[test]
fn a_hosted_site_names_people_by_email_and_keeps_everyone_grants_bounded_by_default() {
    let site = hosted(&with(
        HOSTED,
        &format!(r#"{STORES},"creators":"anyone admitted""#),
    ));
    assert_eq!(identity(&site), &Identity::Email);
    let sharing = site.sharing.expect("the share API");
    assert!(!sharing.everyone_signed_in);
    let named = hosted(&with(HOSTED, r#""identity":"email""#));
    assert_eq!(identity(&named), &Identity::Email);
}

#[test]
fn a_username_claim_names_people_by_username_in_the_oidc_sign_in() {
    let site = hosted(&with(HOSTED, USERNAMES));
    assert_eq!(identity(&site).kind(), IdentityKind::Username);
    let Identity::Username(claim) = identity(&site) else {
        panic!("the username kind");
    };
    assert_eq!(claim.as_str(), "preferred_username");
}

#[test]
fn a_malformed_hosted_identity_is_refused_naming_the_key() {
    for bad in [
        r#""identity":"username""#,
        r#""identity":7"#,
        r#""identity":{"username_claim":""}"#,
        r#""identity":{"username_claim":"a b"}"#,
        r#""identity":{"claim":"preferred_username"}"#,
        r#""identity":{"username_claim":"sub","extra":1}"#,
    ] {
        let message = refusal(&with(HOSTED, bad));
        assert!(message.contains("hosted.identity"), "{bad}: {message}");
    }
}

#[test]
fn a_proxy_identity_claim_beside_a_username_claim_is_refused() {
    let both =
        with(PROXY, USERNAMES).replacen("\"admit\"", "\"identity_claim\":\"upn\",\"admit\"", 1);
    let message = refusal(&both);
    assert!(message.contains("proxy.identity_claim"), "{message}");
    assert!(message.contains("username_claim"), "{message}");
    // The twins: either alone is accepted.
    let claim_alone = PROXY
        .replacen("\"admit\"", "\"identity_claim\":\"upn\",\"admit\"", 1)
        .replace(r#""email":"ana""#, r#""email":"ana@example.com""#);
    assert_eq!(identity(&hosted(&claim_alone)), &Identity::Email);
    let usernames = hosted(&with(PROXY, USERNAMES));
    assert_eq!(identity(&usernames).kind(), IdentityKind::Username);
}

#[test]
fn a_proxy_list_entry_names_a_username_only_under_the_username_kind() {
    let message = refusal(PROXY);
    assert!(message.contains("proxy.admit"), "{message}");
    assert!(parse(&with(PROXY, USERNAMES)).is_ok());
}

#[test]
fn everyone_signed_in_reaches_the_share_api_and_needs_its_stores() {
    let on = hosted(&with(
        HOSTED,
        &format!(r#"{STORES},"creators":"anyone admitted","everyone_signed_in":true"#),
    ));
    assert!(on.sharing.expect("the share API").everyone_signed_in);
    let orphan = refusal(&with(HOSTED, r#""everyone_signed_in":true"#));
    assert!(orphan.contains("hosted.stores"), "{orphan}");
    let not_a_bool = refusal(&with(
        HOSTED,
        &format!(r#"{STORES},"creators":"anyone admitted","everyone_signed_in":"yes""#),
    ));
    assert!(!not_a_bool.is_empty());
}

#[test]
fn under_the_username_kind_names_and_creators_hold_usernames_and_refuse_addresses() {
    let site = hosted(&with(
        HOSTED,
        &format!(
            r#"{USERNAMES},{STORES},"creators":["ana","@example.com"],"names":{{"boss":"ana"}}"#
        ),
    ));
    let sharing = site.sharing.expect("the share API");
    let ana = Person::try_from("ana").unwrap();
    assert_eq!(
        block_on(sharing.recipients.resolve("boss")),
        Ok(ana.clone())
    );
    assert_eq!(
        block_on(sharing.recipients.resolve("bo")),
        Ok(Person::try_from("bo").unwrap())
    );
    assert_eq!(
        block_on(sharing.recipients.resolve("bo@example.com")),
        Err(RecipientError::NotAUsername)
    );
    for bad in [
        format!(r#"{USERNAMES},{STORES},"creators":["ana@example.com"]"#),
        format!(
            r#"{USERNAMES},{STORES},"creators":"anyone admitted","names":{{"boss":"ana@example.com"}}"#
        ),
    ] {
        assert!(parse(&with(HOSTED, &bad)).is_err(), "{bad}");
    }
    // The twin: the email kind keeps taking addresses.
    let emails = hosted(&with(
        HOSTED,
        &format!(r#"{STORES},"creators":["ana@example.com"],"names":{{"boss":"ana@example.com"}}"#),
    ));
    let recipients = emails.sharing.expect("the share API").recipients;
    assert_eq!(
        block_on(recipients.resolve("boss")),
        Ok(Person::try_from("ana@example.com").unwrap())
    );
}

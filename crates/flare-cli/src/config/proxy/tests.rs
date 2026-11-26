//! The `proxy` section: a hosted site takes exactly one of `oidc` and
//! `proxy`, and each refused value names its key. Each refusal has a twin
//! that shows the admitted case.

use super::super::{Problem, SignIn, Site, parse};

const PROXY: &str = r#"{"hosted":{"origin":"https://flare.example.com","listen":"0.0.0.0:8080"},"proxy":{"issuer":"https://sso.example.com","audience":"flare","jwks_url":"https://sso.example.com/.well-known/jwks.json","cookie":"proxy_token","algorithms":["RS256","ES256"],"admit":{"domain":"example.com","reach":"everything"},"sign_in_url":"https://sso.example.com/sign-in"}}"#;

const OIDC: &str = r#""oidc":{"issuer":"https://accounts.google.com","client_id":"abc","client_secret_file":"/run/secrets/google","admit":{"domain":"example.com","reach":"everything"}}"#;

fn refusal(text: &str) -> String {
    match parse(text) {
        Err(Problem::Invalid(message)) => message,
        Ok(_) => panic!("accepted: {text}"),
    }
}

#[test]
fn a_hosted_section_with_a_proxy_section_signs_people_in_through_the_proxy() {
    let Site::Hosted(hosted) = parse(PROXY).unwrap().site else {
        panic!("not a hosted site");
    };
    let SignIn::Proxy(proxy) = &hosted.sign_in else {
        panic!("not a proxy sign-in");
    };
    assert_eq!(proxy.cookie().as_str(), "proxy_token");
    assert_eq!(
        proxy.sign_in_url().map(|link| link.as_str()),
        Some("https://sso.example.com/sign-in")
    );
    assert!(proxy.sign_out_url().is_none());
}

#[test]
fn a_hosted_section_with_both_or_neither_sign_in_is_refused() {
    let both = PROXY.replacen("\"proxy\":", &format!("{OIDC},\"proxy\":"), 1);
    let message = refusal(&both);
    assert!(
        message.contains("exactly one of oidc and proxy"),
        "{message}"
    );
    let neither = r#"{"hosted":{"origin":"https://flare.example.com","listen":"0.0.0.0:8080"}}"#;
    let message = refusal(neither);
    assert!(
        message.contains("oidc") && message.contains("proxy"),
        "{message}"
    );
}

#[test]
fn a_proxy_section_without_a_hosted_section_is_refused() {
    let alone = PROXY.replacen(
        r#""hosted":{"origin":"https://flare.example.com","listen":"0.0.0.0:8080"},"#,
        "",
        1,
    );
    assert!(refusal(&alone).contains("hosted"));
}

#[test]
fn each_refused_proxy_value_names_its_key() {
    for (from, to, key) in [
        ("[\"RS256\",\"ES256\"]", "[\"HS256\"]", "proxy.algorithms"),
        ("[\"RS256\",\"ES256\"]", "[\"none\"]", "proxy.algorithms"),
        ("[\"RS256\",\"ES256\"]", "[]", "proxy.algorithms"),
        (
            "https://sso.example.com/.well-known/jwks.json",
            "http://sso.example.com/jwks",
            "proxy.jwks_url",
        ),
        ("\"proxy_token\"", "\"a;b\"", "proxy.cookie"),
        (
            "\"audience\":\"flare\"",
            "\"audience\":\"\"",
            "proxy.audience",
        ),
        (
            "\"issuer\":\"https://sso.example.com\"",
            "\"issuer\":\"\"",
            "proxy.issuer",
        ),
        (
            "https://sso.example.com/sign-in",
            "http://sso.example.com/sign-in",
            "proxy.sign_in_url",
        ),
        (
            "\"domain\":\"example.com\"",
            "\"domain\":\"\"",
            "proxy.admit",
        ),
    ] {
        let text = PROXY.replacen(from, to, 1);
        let message = refusal(&text);
        assert!(message.contains(key), "{key}: {message}");
    }
}

#[test]
fn an_identity_claim_and_a_sign_out_address_are_read_and_an_unknown_key_refused() {
    let text = PROXY.replacen(
        "\"cookie\"",
        "\"identity_claim\":\"upn\",\"sign_out_url\":\"https://sso.example.com/out\",\"cookie\"",
        1,
    );
    assert!(parse(&text).is_ok());
    let unknown = PROXY.replacen(
        "\"cookie\"",
        "\"header\":\"X-Forwarded-Email\",\"cookie\"",
        1,
    );
    assert!(matches!(parse(&unknown), Err(Problem::Invalid(_))));
}

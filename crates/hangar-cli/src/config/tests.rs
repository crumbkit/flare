use super::{Problem, parse};

const DOMAIN: &str = r#"{"oidc":{"issuer":"https://accounts.google.com","client_id":"abc.apps.googleusercontent.com","admit":{"domain":"example.com","reach":"everything"}}}"#;

const EMAILS: &str = r#"{"oidc":{"issuer":"https://accounts.google.com","client_id":"abc","admit":{"emails":[{"email":"a@example.com","principal":"google:a","reach":"everything"},{"email":"b@example.com","principal":"google:b","reach":[{"page":"task-card"},{"binding":"kitchen"},{"record":"/tasks/tap"}]}]}}}"#;

#[test]
fn an_empty_object_means_loopback_only() {
    assert!(parse("{}").unwrap().oidc.is_none());
}

#[test]
fn a_domain_rule_and_an_email_list_each_give_an_oidc_config() {
    assert!(parse(DOMAIN).unwrap().oidc.is_some());
    assert!(parse(EMAILS).unwrap().oidc.is_some());
}

#[test]
fn an_unknown_key_is_refused_at_every_level() {
    for text in [
        r#"{"colour":"blue"}"#,
        &DOMAIN.replacen("\"issuer\"", "\"extra\":1,\"issuer\"", 1),
        &DOMAIN.replacen("\"reach\"", "\"extra\":1,\"reach\"", 1),
        &EMAILS.replacen("\"principal\"", "\"extra\":1,\"principal\"", 1),
    ] {
        assert!(matches!(parse(text), Err(Problem::Invalid(_))), "{text}");
    }
}

#[test]
fn a_client_secret_is_refused_with_the_keychain_service_and_without_its_value() {
    let text = DOMAIN.replacen(
        "\"issuer\"",
        "\"client_secret\":\"s3cret-value\",\"issuer\"",
        1,
    );
    let Err(Problem::Invalid(message)) = parse(&text) else {
        panic!("a client_secret key was accepted");
    };
    assert!(message.contains("hangar-oidc-google"), "{message}");
    assert!(!message.contains("s3cret-value"), "{message}");
    let top = r#"{"client_secret":"s3cret-value"}"#;
    let Err(Problem::Invalid(message)) = parse(top) else {
        panic!("a client_secret key was accepted");
    };
    assert!(!message.contains("s3cret-value"), "{message}");
}

#[test]
fn values_that_the_oidc_types_refuse_are_refused() {
    for text in [
        DOMAIN.replace("https://accounts.google.com", "http://accounts.google.com"),
        DOMAIN.replace("\"example.com\"", "\"\""),
        EMAILS.replace("a@example.com", "not-an-email"),
        EMAILS.replace("{\"page\":\"task-card\"}", "{\"page\":\"../x\"}"),
        EMAILS.replace(
            "{\"page\":\"task-card\"}",
            "{\"page\":\"a\",\"binding\":\"b\"}",
        ),
        EMAILS.replace("{\"page\":\"task-card\"}", "{}"),
        DOMAIN.replace("\"everything\"", "\"nothing\""),
    ] {
        assert!(matches!(parse(&text), Err(Problem::Invalid(_))), "{text}");
    }
}

#[test]
fn text_that_is_not_json_is_invalid() {
    assert!(matches!(parse("{ not json"), Err(Problem::Invalid(_))));
    assert!(matches!(parse("[]"), Err(Problem::Invalid(_))));
}

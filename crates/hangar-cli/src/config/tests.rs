use super::{Problem, Secret, Site, parse};

const DOMAIN: &str = r#"{"oidc":{"issuer":"https://accounts.google.com","client_id":"abc.apps.googleusercontent.com","admit":{"domain":"example.com","reach":"everything"}}}"#;

const EMAILS: &str = r#"{"oidc":{"issuer":"https://accounts.google.com","client_id":"abc","admit":{"emails":[{"email":"a@example.com","principal":"google:a","reach":"everything"},{"email":"b@example.com","principal":"google:b","reach":[{"page":"task-card"},{"binding":"kitchen"},{"record":"/tasks/tap"}]}]}}}"#;

#[test]
fn an_empty_object_means_loopback_only() {
    assert!(matches!(
        parse("{}").unwrap().site,
        Site::Local { oidc: None }
    ));
}

#[test]
fn a_domain_rule_and_an_email_list_each_give_an_oidc_config() {
    for text in [DOMAIN, EMAILS] {
        assert!(matches!(
            parse(text).unwrap().site,
            Site::Local { oidc: Some(_) }
        ));
    }
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

const HOSTED: &str = r#"{"hosted":{"origin":"https://Hangar.Example.com","listen":"0.0.0.0:8080"},"oidc":{"issuer":"https://accounts.google.com","client_id":"abc","client_secret_file":"/run/secrets/google","admit":{"domain":"example.com","reach":"everything"}}}"#;

fn invalid(text: &str) -> String {
    match parse(text) {
        Err(Problem::Invalid(message)) => message,
        Ok(_) => panic!("{text} was accepted"),
    }
}

#[test]
fn a_hosted_section_parses_with_its_origin_listen_address_secret_file_and_callback() {
    let Site::Hosted(hosted) = parse(HOSTED).unwrap().site else {
        panic!("a hosted config");
    };
    assert_eq!(hosted.origin.as_str(), "https://hangar.example.com");
    assert_eq!(hosted.listen, "0.0.0.0:8080".parse().unwrap());
    assert_eq!(
        hosted.secret_file,
        std::path::PathBuf::from("/run/secrets/google")
    );
    assert_eq!(
        hosted.oidc.redirect_uri().as_str(),
        "https://hangar.example.com/auth/oidc/callback"
    );
}

#[test]
fn an_http_origin_or_one_with_a_path_is_refused_naming_hosted_origin() {
    for origin in [
        "http://hangar.example.com",
        "https://hangar.example.com/",
        "https://hangar.example.com/hangar",
    ] {
        let message = invalid(&HOSTED.replace("https://Hangar.Example.com", origin));
        assert!(message.contains("hosted.origin"), "{origin}: {message}");
    }
}

#[test]
fn a_listen_value_that_is_not_an_address_and_port_is_refused_naming_hosted_listen() {
    for listen in ["8080", "localhost:8080", "0.0.0.0"] {
        let message = invalid(&HOSTED.replace("0.0.0.0:8080", listen));
        assert!(message.contains("hosted.listen"), "{listen}: {message}");
    }
    assert!(parse(&HOSTED.replace("0.0.0.0:8080", "[::]:8080")).is_ok());
}

#[test]
fn a_hosted_section_without_oidc_is_refused_naming_oidc() {
    let message =
        invalid(r#"{"hosted":{"origin":"https://hangar.example.com","listen":"0.0.0.0:8080"}}"#);
    assert!(message.contains("oidc"), "{message}");
}

#[test]
fn a_hosted_section_without_a_client_secret_file_is_refused_naming_the_key() {
    let message = invalid(&HOSTED.replace(r#""client_secret_file":"/run/secrets/google","#, ""));
    assert!(message.contains("oidc.client_secret_file"), "{message}");
}

#[test]
fn a_relative_client_secret_file_is_refused_and_an_absolute_one_is_admitted() {
    let message = invalid(&HOSTED.replace("/run/secrets/google", "secrets/google"));
    assert!(message.contains("oidc.client_secret_file"), "{message}");
    assert!(parse(HOSTED).is_ok());
}

#[test]
fn an_unknown_key_in_the_hosted_section_is_refused() {
    let text = HOSTED.replace(r#""listen""#, r#""tls":true,"listen""#);
    assert!(matches!(parse(&text), Err(Problem::Invalid(_))));
}

#[test]
fn a_local_config_may_name_a_client_secret_file_and_otherwise_uses_the_keychain() {
    let Site::Local { oidc: Some(file) } = parse(&DOMAIN.replace(
        r#""client_id""#,
        r#""client_secret_file":"/run/secrets/google","client_id""#,
    ))
    .unwrap()
    .site
    else {
        panic!("a local config with OpenID Connect");
    };
    assert_eq!(
        file.secret,
        Secret::File(std::path::PathBuf::from("/run/secrets/google"))
    );
    assert_eq!(
        file.config.redirect_uri().as_str(),
        "http://localhost:8740/auth/oidc/callback"
    );
    let Site::Local {
        oidc: Some(keychain),
    } = parse(DOMAIN).unwrap().site
    else {
        panic!("a local config with OpenID Connect");
    };
    assert_eq!(keychain.secret, Secret::Keychain);
}

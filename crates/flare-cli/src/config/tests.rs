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
    assert!(message.contains("flare-oidc-google"), "{message}");
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

const HOSTED: &str = r#"{"hosted":{"origin":"https://Flare.Example.com","listen":"0.0.0.0:8080"},"oidc":{"issuer":"https://accounts.google.com","client_id":"abc","client_secret_file":"/run/secrets/google","admit":{"domain":"example.com","reach":"everything"}}}"#;

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
    assert_eq!(hosted.origin.as_str(), "https://flare.example.com");
    assert_eq!(hosted.listen, "0.0.0.0:8080".parse().unwrap());
    assert_eq!(
        hosted.secret_file,
        std::path::PathBuf::from("/run/secrets/google")
    );
    assert_eq!(
        hosted.oidc.redirect_uri().as_str(),
        "https://flare.example.com/auth/oidc/callback"
    );
}

#[test]
fn an_http_origin_or_one_with_a_path_is_refused_naming_hosted_origin() {
    for origin in [
        "http://flare.example.com",
        "https://flare.example.com/",
        "https://flare.example.com/flare",
    ] {
        let message = invalid(&HOSTED.replace("https://Flare.Example.com", origin));
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
        invalid(r#"{"hosted":{"origin":"https://flare.example.com","listen":"0.0.0.0:8080"}}"#);
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

const STORES: &str =
    r#""stores":{"region":"eu-west-1","table":"flare-shares","bucket":"flare-files"}"#;

/// The hosted config with `extra` added to its hosted section.
fn hosted_with(extra: &str) -> String {
    HOSTED.replacen("\"origin\"", &format!("{extra},\"origin\""), 1)
}

/// The hosted config with `extra` in its hosted section, admitting anyone.
fn anyone_with(extra: &str) -> String {
    hosted_with(extra).replace(
        r#"{"domain":"example.com","reach":"everything"}"#,
        r#"{"anyone":true,"reach":"everything"}"#,
    )
}

#[test]
fn a_hosted_section_with_stores_serves_the_share_api_and_every_admitted_person_may_share() {
    let Site::Hosted(hosted) = parse(&hosted_with(STORES)).unwrap().site else {
        panic!("a hosted config");
    };
    let sharing = hosted.sharing.expect("the share API");
    assert_eq!(sharing.stores.table().as_str(), "flare-shares");
    assert_eq!(sharing.creators, flare_core::Creators::Everyone);
}

#[test]
fn a_hosted_section_without_stores_serves_no_share_api() {
    let Site::Hosted(hosted) = parse(HOSTED).unwrap().site else {
        panic!("a hosted config");
    };
    assert!(hosted.sharing.is_none());
}

#[test]
fn creators_limit_who_may_share() {
    let text = hosted_with(&format!(r#"{STORES},"creators":["ana@example.org"]"#));
    let Site::Hosted(hosted) = parse(&text).unwrap().site else {
        panic!("a hosted config");
    };
    let creators = hosted.sharing.expect("the share API").creators;
    let ana = flare_core::Email::try_from("ana@example.org").unwrap();
    assert_eq!(creators, flare_core::Creators::only(vec![ana]));
}

#[test]
fn admitting_anyone_without_a_creators_list_is_refused_naming_hosted_creators() {
    let message = invalid(&anyone_with(STORES));
    assert!(message.contains("hosted.creators"), "{message}");
}

#[test]
fn admitting_anyone_with_a_creators_list_is_admitted() {
    let text = anyone_with(&format!(r#"{STORES},"creators":["ana@example.org"]"#));
    assert!(matches!(parse(&text).unwrap().site, Site::Hosted(_)));
}

#[test]
fn creators_without_stores_are_refused_naming_hosted_stores() {
    let message = invalid(&hosted_with(r#""creators":["ana@example.org"]"#));
    assert!(message.contains("hosted.stores"), "{message}");
}

#[test]
fn a_creator_that_is_not_an_address_is_refused() {
    let message = invalid(&hosted_with(&format!(r#"{STORES},"creators":["ana"]"#)));
    assert!(message.contains("hosted.creators"), "{message}");
}

#[test]
fn a_credential_in_the_stores_section_is_refused() {
    let text = hosted_with(
        r#""stores":{"region":"eu-west-1","table":"flare-shares","bucket":"flare-files","secret_access_key":"x"}"#,
    );
    assert!(matches!(parse(&text), Err(Problem::Invalid(_))));
}

fn creators_of(extra: &str) -> flare_core::Creators {
    let hosted = match parse(&hosted_with(&format!("{STORES},{extra}")))
        .unwrap()
        .site
    {
        Site::Hosted(hosted) => hosted,
        Site::Local { .. } => panic!("not hosted"),
    };
    hosted.sharing.expect("the share API").creators
}

fn person(address: &str) -> flare_core::Principal {
    flare_core::Principal::with_email(
        flare_core::PrincipalId::try_from(address).unwrap(),
        flare_core::Email::try_from(address).unwrap(),
    )
}

#[test]
fn a_creators_domain_entry_admits_that_domain_and_an_address_entry_still_works() {
    let creators = creators_of(r#""creators":["@example.com","jo@other.example"]"#);
    assert!(creators.admits(&person("ana@example.com")));
    assert!(creators.admits(&person("jo@other.example")));
    assert!(!creators.admits(&person("ana@other.example")));
}

#[test]
fn the_creators_value_anyone_admitted_lets_every_admitted_person_share() {
    let creators = creators_of(r#""creators":"anyone admitted""#);
    assert_eq!(creators, flare_core::Creators::Everyone);
}

#[test]
fn anyone_admitted_is_accepted_under_an_anyone_admit_rule() {
    let text = anyone_with(&format!(r#"{STORES},"creators":"anyone admitted""#));
    assert!(parse(&text).is_ok());
}

#[test]
fn a_creators_value_that_is_not_a_list_or_anyone_admitted_is_refused_naming_the_key() {
    for extra in [
        r#""creators":"everyone""#,
        r#""creators":["anyone admitted"]"#,
    ] {
        let message = invalid(&hosted_with(&format!("{STORES},{extra}")));
        assert!(message.contains("hosted.creators"), "{message}");
        assert!(message.contains("anyone admitted"), "{message}");
    }
}

#[test]
fn a_bad_creators_domain_entry_is_refused_naming_the_key() {
    for entry in ["@", "@*.example.com", "example.com"] {
        let message = invalid(&hosted_with(&format!(r#"{STORES},"creators":["{entry}"]"#)));
        assert!(message.contains("hosted.creators"), "{message}");
    }
}

#[test]
fn names_map_usernames_to_addresses_for_the_share_api() {
    use flare_core::Recipients as _;
    let text = hosted_with(&format!(r#"{STORES},"names":{{"ana":"ana@example.com"}}"#));
    let Site::Hosted(hosted) = parse(&text).unwrap().site else {
        panic!("not hosted");
    };
    let recipients = hosted.sharing.expect("the share API").recipients;
    let ana = flare_core::contract::block_on(recipients.resolve("ana")).unwrap();
    assert_eq!(ana.as_str(), "ana@example.com");
}

#[test]
fn a_bad_names_entry_is_refused_naming_the_key() {
    for names in [
        r#"{"ana":"not an address"}"#,
        r#"{"":"ana@example.com"}"#,
        r#"{"a@b":"ana@example.com"}"#,
        r#"{"Ana":"ana@example.com"}"#,
        r#"{"team:x":"ana@example.com"}"#,
    ] {
        let message = invalid(&hosted_with(&format!(r#"{STORES},"names":{names}"#)));
        assert!(message.contains("hosted.names"), "{message}");
    }
}

#[test]
fn names_without_stores_are_refused_naming_hosted_stores() {
    let message = invalid(&hosted_with(r#""names":{"ana":"ana@example.com"}"#));
    assert!(message.contains("hosted.stores"), "{message}");
}

fn open_to_anyone(text: &str) -> bool {
    let Site::Hosted(hosted) = parse(text).unwrap().site else {
        panic!("not hosted");
    };
    hosted.sharing.expect("the share API").open_to_anyone
}

#[test]
fn anyone_admitted_under_an_anyone_admit_rule_is_marked_open_to_anyone() {
    let text = anyone_with(&format!(r#"{STORES},"creators":"anyone admitted""#));
    assert!(open_to_anyone(&text));
}

#[test]
fn other_creators_settings_are_not_marked_open_to_anyone() {
    let listed = anyone_with(&format!(r#"{STORES},"creators":["ana@example.org"]"#));
    assert!(!open_to_anyone(&listed));
    let domain_rule = hosted_with(&format!(r#"{STORES},"creators":"anyone admitted""#));
    assert!(!open_to_anyone(&domain_rule));
}

#[test]
fn log_metrics_is_off_when_the_config_does_not_set_it() {
    assert!(!parse("{}").unwrap().metrics);
    assert!(!parse(DOMAIN).unwrap().metrics);
    assert!(!parse(r#"{"log":{}}"#).unwrap().metrics);
}

#[test]
fn log_metrics_true_turns_the_metric_lines_on_for_the_local_and_the_hosted_site() {
    assert!(parse(r#"{"log":{"metrics":true}}"#).unwrap().metrics);
    assert!(!parse(r#"{"log":{"metrics":false}}"#).unwrap().metrics);
    let hosted = r#"{"log":{"metrics":true},"hosted":{"origin":"https://flare.example.com","listen":"127.0.0.1:8080"},"oidc":{"issuer":"https://accounts.google.com","client_id":"x","client_secret_file":"/run/secret","admit":{"domain":"example.com","reach":"everything"}}}"#;
    let config = parse(hosted).unwrap();
    assert!(config.metrics);
    assert!(matches!(config.site, Site::Hosted(_)));
}

#[test]
fn a_misspelt_log_key_or_a_metrics_value_that_is_not_a_boolean_is_refused_naming_it() {
    for (text, named) in [
        (r#"{"log":{"metric":true}}"#, "metric"),
        (r#"{"logs":{"metrics":true}}"#, "logs"),
        (r#"{"log":{"metrics":"yes"}}"#, "boolean"),
    ] {
        let Err(Problem::Invalid(message)) = parse(text) else {
            panic!("{text} was accepted");
        };
        assert!(message.contains(named), "{message}");
    }
}

#[test]
fn a_get_flare_address_is_read_with_the_share_api() {
    let text = hosted_with(&format!(
        r#"{STORES},"get_flare":"https://example.com/flare""#
    ));
    let Site::Hosted(hosted) = parse(&text).unwrap().site else {
        panic!("a hosted config");
    };
    let get_flare = hosted.sharing.expect("the share API").get_flare;
    assert_eq!(
        get_flare.as_ref().map(flare_server::GetFlare::as_str),
        Some("https://example.com/flare")
    );
    let Site::Hosted(plain) = parse(&hosted_with(STORES)).unwrap().site else {
        panic!("a hosted config");
    };
    assert!(plain.sharing.expect("the share API").get_flare.is_none());
}

#[test]
fn a_get_flare_address_that_is_not_an_https_url_with_a_host_is_refused_naming_the_key() {
    for address in [
        "http://example.com/flare",
        "/flare",
        "example.com/flare",
        "not a url",
    ] {
        let message = invalid(&hosted_with(&format!(
            r#"{STORES},"get_flare":"{address}""#
        )));
        assert!(message.contains("hosted.get_flare"), "{address}: {message}");
        assert!(!message.contains(address), "{address}: {message}");
    }
}

#[test]
fn a_get_flare_address_without_stores_is_refused_naming_hosted_stores() {
    let message = invalid(&hosted_with(r#""get_flare":"https://example.com/flare""#));
    assert!(message.contains("hosted.get_flare"), "{message}");
    assert!(message.contains("hosted.stores"), "{message}");
}

use super::{OriginError, PublicOrigin};

fn origin(text: &str) -> Result<PublicOrigin, OriginError> {
    PublicOrigin::try_from(text)
}

#[test]
fn an_https_origin_is_admitted_with_its_authority_and_callback() {
    let public = origin("https://flare.example.com").unwrap();
    assert_eq!(public.as_str(), "https://flare.example.com");
    assert_eq!(public.authority(), "flare.example.com");
    assert_eq!(
        public.redirect_uri(),
        "https://flare.example.com/auth/oidc/callback"
    );
    let ported = origin("https://flare.example.com:8443").unwrap();
    assert_eq!(ported.authority(), "flare.example.com:8443");
}

#[test]
fn mixed_case_and_port_443_normalise_and_an_internationalised_name_becomes_ascii() {
    let public = origin("HTTPS://Flare.Example.COM:443").unwrap();
    assert_eq!(public.as_str(), "https://flare.example.com");
    assert_eq!(public.authority(), "flare.example.com");
    let idn = origin("https://b\u{fc}cher.example").unwrap();
    assert_eq!(idn.as_str(), "https://xn--bcher-kva.example");
    assert_eq!(idn.authority(), "xn--bcher-kva.example");
}

#[test]
fn a_trailing_slash_a_path_a_query_a_fragment_a_user_or_the_http_scheme_is_refused() {
    for text in [
        "https://flare.example.com/",
        "https://flare.example.com/pages",
        "https://flare.example.com?x=1",
        "https://flare.example.com?",
        "https://flare.example.com#top",
        "https://ada@flare.example.com",
        "https://ada:pw@flare.example.com",
        "http://flare.example.com",
        "ftp://flare.example.com",
        "https://",
        "flare.example.com",
        "",
    ] {
        assert!(origin(text).is_err(), "{text}");
    }
    assert!(origin("https://flare.example.com").is_ok());
}

#[cfg(feature = "testing")]
#[test]
fn the_loopback_test_origin_admits_http_on_localhost_only() {
    let public = PublicOrigin::loopback_http("http://localhost:8123").unwrap();
    assert_eq!(public.authority(), "localhost:8123");
    assert_eq!(public.as_str(), "http://localhost:8123");
    for text in [
        "http://flare.example.com:8123",
        "http://localhost:8123/",
        "https://localhost:8123",
    ] {
        assert!(PublicOrigin::loopback_http(text).is_err(), "{text}");
    }
}

#[test]
fn two_origins_are_on_separate_domains_unless_one_host_is_the_other_or_under_it() {
    let site = origin("https://flare.example.com").unwrap();
    for same in [
        "https://flare.example.com:8443",
        "https://content.flare.example.com",
        "https://example.com",
    ] {
        let content = origin(same).unwrap();
        assert!(!content.is_separate_domain_from(&site), "{same}");
        assert!(!site.is_separate_domain_from(&content), "{same}");
    }
    for separate in [
        "https://flareusercontent.com",
        "https://contentflare.example.com",
        "https://[::1]:8443",
    ] {
        let content = origin(separate).unwrap();
        assert!(content.is_separate_domain_from(&site), "{separate}");
    }
    let v6 = origin("https://[::1]").unwrap();
    assert!(
        !origin("https://[::1]:8443")
            .unwrap()
            .is_separate_domain_from(&v6)
    );
}

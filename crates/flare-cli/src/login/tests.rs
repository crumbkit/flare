use super::origin;

#[test]
fn an_https_server_or_a_loopback_http_server_gives_its_origin() {
    assert_eq!(
        origin("https://flare.example.org").ok(),
        Some("https://flare.example.org".to_owned())
    );
    assert_eq!(
        origin("https://flare.example.org:8443/").ok(),
        Some("https://flare.example.org:8443".to_owned())
    );
    assert_eq!(
        origin("http://127.0.0.1:4000").ok(),
        Some("http://127.0.0.1:4000".to_owned())
    );
    assert_eq!(
        origin("http://localhost:4000").ok(),
        Some("http://localhost:4000".to_owned())
    );
}

#[test]
fn a_server_that_could_leak_the_token_or_names_more_than_an_origin_is_refused() {
    for refused in [
        "http://flare.example.org",
        "ftp://flare.example.org",
        "https://user:pass@flare.example.org",
        "https://flare.example.org/path",
        "https://flare.example.org/?q=1",
        "https://flare.example.org/#top",
        "flare.example.org",
    ] {
        assert!(origin(refused).is_err(), "{refused}");
    }
}

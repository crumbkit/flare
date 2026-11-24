use super::{Found, Need, ROUTES, Sites, find};
use crate::site::SiteKind;
use flare_core::Access;
use http::Method;

#[test]
fn every_route_names_the_sites_that_serve_it() {
    for route in &ROUTES {
        assert!(
            route.sites.serves(SiteKind::Local) || route.sites.serves(SiteKind::Hosted),
            "{} {}",
            route.method,
            route.pattern
        );
    }
}

#[test]
fn no_get_route_writes_on_either_site() {
    for route in ROUTES.iter().filter(|route| route.method == Method::GET) {
        assert_ne!(route.access, Some(Access::Write), "{}", route.pattern);
        assert!(!route.handler.writes(), "{}", route.pattern);
    }
}

#[test]
fn no_board_write_serves_the_hosted_site_and_its_other_methods_are_api_routes_that_read_no_cookie()
{
    for route in &ROUTES {
        let writes = route.handler.writes() || route.access == Some(Access::Write);
        if writes {
            assert_eq!(
                route.sites,
                Sites::Local,
                "{} {}",
                route.method,
                route.pattern
            );
        }
        if route.method != Method::GET && route.sites.serves(SiteKind::Hosted) {
            assert!(
                route.pattern.starts_with("/api/"),
                "{} {}",
                route.method,
                route.pattern
            );
            let exchange = route.pattern == "/api/tokens/exchange";
            assert!(
                route.credential == Need::Token || (exchange && route.credential == Need::Nothing),
                "{} {}",
                route.method,
                route.pattern
            );
        }
    }
    assert!(ROUTES.iter().any(|route| route.handler.writes()));
}

#[test]
fn the_share_api_is_served_only_on_the_hosted_site() {
    for (method, path) in [
        (Method::POST, "/api/shares"),
        (Method::GET, "/api/shares"),
        (Method::GET, "/api/activity"),
        (Method::GET, "/api/search"),
        (Method::GET, "/api/search/fields"),
        (Method::POST, "/api/tokens/exchange"),
        (Method::GET, "/cli/login"),
    ] {
        assert!(
            matches!(find(&method, path, SiteKind::Hosted), Found::Route(..)),
            "{path}"
        );
        assert!(
            matches!(find(&method, path, SiteKind::Local), Found::Nothing),
            "{path}"
        );
    }
}

#[test]
fn the_hosted_site_serves_its_seven_routes_and_none_of_the_local_ones() {
    let hosted = [
        "/up",
        "/flare/style.css",
        "/",
        "/flare/",
        "/pages/home",
        "/auth/oidc/start",
        "/auth/oidc/callback",
    ];
    for path in hosted {
        assert!(
            matches!(find(&Method::GET, path, SiteKind::Hosted), Found::Route(..)),
            "{path}"
        );
    }
    for path in [
        "/b/kitchen",
        "/ready",
        "/launch",
        "/launch/probe",
        "/auth/oidc/finish",
    ] {
        assert!(
            matches!(find(&Method::GET, path, SiteKind::Hosted), Found::Nothing),
            "{path}"
        );
        assert!(
            matches!(find(&Method::GET, path, SiteKind::Local), Found::Route(..)),
            "{path}"
        );
    }
    for path in ["/b/kitchen", "/launch/codes"] {
        assert!(matches!(
            find(&Method::POST, path, SiteKind::Hosted),
            Found::Nothing
        ));
    }
    assert!(matches!(
        find(&Method::POST, "/pages/home", SiteKind::Hosted),
        Found::MethodNotAllowed
    ));
    for path in ["/auth/oidc/start", "/auth/oidc/callback"] {
        assert!(
            matches!(find(&Method::GET, path, SiteKind::Local), Found::Nothing),
            "{path}"
        );
    }
}

#[test]
fn a_share_id_followed_by_versions_finds_the_publish_route_and_names_the_id() {
    let found = find(
        &Method::POST,
        "/api/shares/abc123/versions",
        SiteKind::Hosted,
    );
    let Found::Route(route, name) = found else {
        panic!("no route: {found:?}");
    };
    assert_eq!(route.pattern, "/api/shares/<name>/versions");
    assert_eq!(name, "abc123");
}

#[test]
fn the_publish_route_takes_one_segment_for_the_id_and_only_post() {
    for path in ["/api/shares//versions", "/api/shares/a/b/versions"] {
        assert!(
            !matches!(
                find(&Method::POST, path, SiteKind::Hosted),
                Found::Route(route, _) if route.pattern.ends_with("/versions")
            ),
            "{path}"
        );
    }
    assert!(matches!(
        find(
            &Method::PUT,
            "/api/shares/abc123/versions",
            SiteKind::Hosted
        ),
        Found::MethodNotAllowed
    ));
    assert!(matches!(
        find(
            &Method::POST,
            "/api/shares/abc123/versions",
            SiteKind::Local
        ),
        Found::Nothing
    ));
}

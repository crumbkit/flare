use super::{Found, ROUTES, Sites, find};
use crate::site::SiteKind;
use hangar_core::Access;
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
fn no_route_with_a_write_handler_or_another_method_than_get_serves_the_hosted_site() {
    for route in &ROUTES {
        let writes = route.handler.writes() || route.access == Some(Access::Write);
        if writes || route.method != Method::GET {
            assert_eq!(
                route.sites,
                Sites::Local,
                "{} {}",
                route.method,
                route.pattern
            );
        }
    }
    assert!(ROUTES.iter().any(|route| route.handler.writes()));
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

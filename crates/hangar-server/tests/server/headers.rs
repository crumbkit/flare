//! Plan tests 2, 3, 4 and 18: the reply headers, the credential each route
//! needs, the Host check on both listeners, and credentialed cross-site
//! requests.

use crate::harness::{BINDING, PAGE, Reply, Server};
use std::collections::BTreeSet;

/// The threat model's policy for hangar's own documents.
pub(crate) const POLICY: &str = "default-src 'none'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'";

/// Asserts that `reply` carries the policy and the six other headers, once
/// each.
pub(crate) fn assert_headers(reply: &Reply, what: &str) {
    for (name, value) in [
        ("content-security-policy", POLICY),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("cache-control", "no-store"),
        ("cross-origin-resource-policy", "same-origin"),
        ("cross-origin-opener-policy", "same-origin"),
        ("x-frame-options", "DENY"),
    ] {
        assert_eq!(
            reply.all(name),
            vec![value],
            "{name} on {what} ({})",
            reply.status
        );
    }
}

/// The path of a route pattern with the fixtures' names filled in.
pub(crate) fn concrete(pattern: &str) -> String {
    let name = if pattern.starts_with("/pages/") {
        PAGE
    } else {
        BINDING
    };
    pattern.replace("<name>", name)
}

#[tokio::test]
async fn every_route_and_every_error_status_carries_the_policy_and_the_six_other_headers() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut seen = BTreeSet::new();
    for (method, pattern) in hangar_server::routes() {
        let path = concrete(pattern);
        let call = server.request(method.as_str(), &path).same_origin();
        for reply in [call.clone().send().await, call.cookie(&cookie).send().await] {
            assert_headers(&reply, &format!("{method} {pattern}"));
            seen.insert(reply.status);
        }
    }
    let errors = [
        server.get("/nowhere").send().await,
        server.request("DELETE", "/up").same_origin().send().await,
        server.get("/pages/Bad..Name").cookie(&cookie).send().await,
        server.get("/up").host("evil.example:80").send().await,
        server.get(&format!("/pages/{PAGE}")).send().await,
        server
            .get("/up")
            .header("X-Filler", &"a".repeat(40 * 1024))
            .send()
            .await,
        server
            .request("POST", &format!("/b/{BINDING}"))
            .cookie(&cookie)
            .same_origin()
            .body("text/plain", b"x".to_vec())
            .send()
            .await,
    ];
    for reply in &errors {
        assert_headers(reply, "an error");
        seen.insert(reply.status);
    }
    for status in [200, 303, 400, 401, 403, 404, 405, 415, 431] {
        assert!(
            seen.contains(&status),
            "no reply had status {status}: {seen:?}"
        );
    }
}

#[tokio::test]
async fn a_client_that_asks_for_json_gets_a_problem_document_with_the_code() {
    let server = Server::start().await;
    let reply = server
        .get("/nowhere")
        .header("Accept", "application/json")
        .send()
        .await;
    assert_eq!(reply.status, 404);
    assert_eq!(
        reply.header("content-type"),
        Some("application/problem+json")
    );
    let problem: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(problem["status"], 404);
    assert_eq!(problem["code"], "request::not_found");
    let html = server.get("/nowhere").send().await;
    assert_eq!(
        html.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(html.code().as_deref(), Some("request::not_found"));
}

/// The routes that answer without a credential.
const OPEN: [&str; 6] = [
    "/up",
    "/ready",
    "/launch",
    "/launch/probe",
    "/auth/oidc/finish",
    "/hangar/style.css",
];

#[tokio::test]
async fn every_route_outside_the_open_list_answers_401_without_a_good_credential() {
    let server = Server::start().await;
    server.seed_binding().await;
    let good = server.sign_in().await;
    let mut open = BTreeSet::new();
    let mut guarded = 0;
    for (method, pattern) in hangar_server::routes() {
        let path = concrete(pattern);
        let call = server.request(method.as_str(), &path).same_origin();
        let none = call.clone().send().await;
        if none.status != 401 {
            open.insert(pattern);
            continue;
        }
        guarded += 1;
        assert_eq!(
            none.code().as_deref(),
            Some("auth::sign_in_needed"),
            "{pattern}"
        );
        let wrong_token = server
            .request(method.as_str(), &path)
            .bearer("not-the-launch-token")
            .send()
            .await;
        assert_eq!(
            wrong_token.status, 401,
            "a wrong token on {method} {pattern}"
        );
        let unknown_session = call
            .clone()
            .cookie("hangar_session=no-such-session")
            .send()
            .await;
        assert_eq!(
            unknown_session.status, 401,
            "an unknown session on {method} {pattern}"
        );
        let admitted = if pattern == "/launch/codes" {
            server
                .request(method.as_str(), &path)
                .bearer(&server.token())
                .send()
                .await
        } else {
            call.cookie(&good).send().await
        };
        assert_ne!(
            admitted.status, 401,
            "the admitted twin of {method} {pattern}"
        );
    }
    assert_eq!(open, OPEN.into_iter().collect::<BTreeSet<_>>());
    assert!(guarded >= 5, "only {guarded} routes need a credential");
}

#[tokio::test]
async fn a_wrong_host_is_refused_with_403_and_the_right_host_is_served() {
    let server = Server::start().await;
    for host in [
        "localhost",
        &format!("localhost:{}", server.port),
        &format!("127.0.0.1:{}", server.port),
        "h-00000000000000000000000000000000.localhost:1",
        &server.authority.to_uppercase(),
    ] {
        let reply = server.get("/up").host(host).send().await;
        assert_eq!(reply.status, 403, "Host {host}");
        assert_eq!(reply.code().as_deref(), Some("request::host"));
    }
    assert_eq!(server.get("/up").send().await.status, 200);
}

#[tokio::test]
async fn the_launch_host_does_not_serve_the_sign_in_hosts_paths() {
    let server = Server::start().await;
    for path in [
        "/auth/oidc/start?return_to=/hangar/",
        "/auth/oidc/callback?state=x",
    ] {
        assert_eq!(server.get(path).send().await.status, 404, "{path}");
    }
}

#[tokio::test]
async fn a_credentialed_cross_site_request_is_refused_unless_it_is_a_navigation() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let page = format!("/pages/{PAGE}");
    for mode in [
        Some("no-cors"),
        Some("cors"),
        Some("same-origin"),
        Some("websocket"),
        None,
    ] {
        for path in ["/hangar/style.css", page.as_str()] {
            let mut call = server
                .get(path)
                .cookie(&cookie)
                .header("Sec-Fetch-Site", "cross-site");
            if let Some(mode) = mode {
                call = call.header("Sec-Fetch-Mode", mode);
            }
            let reply = call.send().await;
            assert_eq!(reply.status, 403, "{path} in mode {mode:?}");
            assert_eq!(reply.code().as_deref(), Some("request::cross_site"));
        }
        let bearer = server
            .get("/hangar/style.css")
            .bearer(&server.token())
            .header("Sec-Fetch-Site", "cross-site")
            .header("Sec-Fetch-Mode", mode.unwrap_or("no-cors"))
            .send()
            .await;
        assert_eq!(
            bearer.status, 403,
            "a bearer token cross-site in mode {mode:?}"
        );
    }
    let no_credential = server
        .get("/hangar/style.css")
        .header("Sec-Fetch-Site", "cross-site")
        .header("Sec-Fetch-Mode", "no-cors")
        .send()
        .await;
    assert_eq!(no_credential.status, 200);
    for (site, mode) in [
        ("cross-site", "navigate"),
        ("same-origin", "no-cors"),
        ("none", "navigate"),
        ("same-site", "no-cors"),
    ] {
        let reply = server
            .get(&page)
            .cookie(&cookie)
            .header("Sec-Fetch-Site", site)
            .header("Sec-Fetch-Mode", mode)
            .send()
            .await;
        assert_eq!(
            reply.status, 200,
            "Sec-Fetch-Site {site}, Sec-Fetch-Mode {mode}"
        );
    }
}

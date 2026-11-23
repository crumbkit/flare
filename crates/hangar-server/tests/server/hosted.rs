//! The hosted site's request checks, routes and headers: the Host, the
//! Origin of a non-GET request, the exact `__Host-` session cookie, the
//! cross-site rule, the 404 routes and HSTS. The sign-in itself is in
//! hosted_sign_in.rs.

use crate::fake_provider::HOME;
use crate::harness::{BINDING, Reply};
use crate::hosted::{Admits, Hosted, SESSION};
use crate::{harness, oidc};
use std::sync::atomic::Ordering;
use std::time::Duration;

/// The seven headers every reply carries, and their values.
const FIXED: [(&str, &str); 7] = [
    ("content-security-policy", "default-src 'none'"),
    ("x-content-type-options", "nosniff"),
    ("referrer-policy", "same-origin"),
    ("cache-control", "no-store"),
    ("cross-origin-resource-policy", "same-origin"),
    ("cross-origin-opener-policy", "same-origin"),
    ("x-frame-options", "DENY"),
];

const HSTS: &str = "max-age=63072000";

fn refused(reply: &Reply) -> (u16, Option<String>) {
    (reply.status, reply.code())
}

#[tokio::test]
async fn a_hosted_server_refuses_a_request_whose_host_is_not_the_origins_and_serves_the_right_host()
{
    let hosted = Hosted::start().await;
    let cookie = hosted.sign_in().await;
    let paths = [
        format!("/pages/{HOME}"),
        "/up".to_owned(),
        format!("/auth/oidc/start?return_to=/pages/{HOME}"),
        "/auth/oidc/callback?state=x&code=y".to_owned(),
    ];
    for path in &paths {
        for host in [
            "evil.example",
            "localhost",
            &format!("127.0.0.1:{}", hosted.port),
            &format!("LOCALHOST:{}", hosted.port),
        ] {
            let reply = hosted.get(path).host(host).cookie(&cookie).send().await;
            assert_eq!(
                refused(&reply),
                (403, Some("request::host".to_owned())),
                "{path} with {host}"
            );
        }
        let missing = hosted.get(path).without("Host").send().await;
        assert_eq!(missing.status, 403, "{path} without a Host");
        let repeated = hosted
            .get(path)
            .header("Host", &hosted.authority)
            .send()
            .await;
        assert_eq!(missing.code().as_deref(), Some("request::host"));
        assert_eq!(
            refused(&repeated),
            (403, Some("request::host".to_owned())),
            "{path} with two Hosts"
        );
    }
    assert_eq!(hosted.read_home(&cookie).await, 200);
    assert_eq!(hosted.get("/up").send().await.status, 200);
}

#[tokio::test]
async fn a_hosted_server_ignores_x_forwarded_host() {
    let hosted = Hosted::start().await;
    let cookie = hosted.sign_in().await;
    let page = format!("/pages/{HOME}");
    let served = hosted
        .get(&page)
        .cookie(&cookie)
        .header("X-Forwarded-Host", "evil.example")
        .send()
        .await;
    assert_eq!(served.status, 200, "{}", served.body);
    let refused_reply = hosted
        .get(&page)
        .host("evil.example")
        .header("X-Forwarded-Host", &hosted.authority)
        .cookie(&cookie)
        .send()
        .await;
    assert_eq!(
        refused(&refused_reply),
        (403, Some("request::host".to_owned()))
    );
}

#[tokio::test]
async fn a_hosted_server_refuses_an_authorization_header_on_every_route_and_serves_without_it() {
    let hosted = Hosted::start().await;
    let cookie = hosted.sign_in().await;
    for path in [
        format!("/pages/{HOME}"),
        format!("/auth/oidc/start?return_to=/pages/{HOME}"),
        "/auth/oidc/callback?state=x&code=y".to_owned(),
    ] {
        for value in ["Bearer abc", "Basic YTpi"] {
            let reply = hosted
                .get(&path)
                .cookie(&cookie)
                .header("Authorization", value)
                .send()
                .await;
            assert_eq!(
                refused(&reply),
                (401, Some("auth::bad_credential".to_owned())),
                "{path} with {value}"
            );
        }
    }
    assert_eq!(hosted.read_home(&cookie).await, 200);
    let start = hosted
        .get(&format!("/auth/oidc/start?return_to=/pages/{HOME}"))
        .send()
        .await;
    assert_eq!(start.status, 303);
}

#[tokio::test]
async fn a_hosted_server_refuses_a_post_from_any_other_origin_and_answers_405_to_its_own() {
    let hosted = Hosted::start().await;
    let cookie = hosted.sign_in().await;
    let page = format!("/pages/{HOME}");
    let other_scheme = hosted.origin.replacen("http://", "https://", 1);
    let suffixed = format!("{}.evil.example", hosted.origin);
    let slashed = format!("{}/", hosted.origin);
    let origins: Vec<Vec<&str>> = vec![
        vec![],
        vec![&hosted.origin, &hosted.origin],
        vec!["null"],
        vec!["https://evil.example"],
        vec![&suffixed],
        vec![&slashed],
        vec![&other_scheme],
    ];
    for sent in &origins {
        let mut call = hosted
            .request("POST", &page)
            .cookie(&cookie)
            .header("Sec-Fetch-Site", "same-origin");
        for origin in sent {
            call = call.header("Origin", origin);
        }
        let reply = call.send().await;
        assert_eq!(
            refused(&reply),
            (403, Some("request::origin".to_owned())),
            "{sent:?}"
        );
    }
    for site in [None, Some("same-site"), Some("cross-site"), Some("none")] {
        let mut call = hosted
            .request("POST", &page)
            .cookie(&cookie)
            .header("Origin", &hosted.origin);
        if let Some(site) = site {
            call = call.header("Sec-Fetch-Site", site);
        }
        let reply = call.send().await;
        assert_eq!(
            refused(&reply),
            (403, Some("request::origin".to_owned())),
            "{site:?}"
        );
    }
    let own = hosted
        .request("POST", &page)
        .cookie(&cookie)
        .same_origin()
        .send()
        .await;
    assert_eq!(
        refused(&own),
        (405, Some("request::method_not_allowed".to_owned()))
    );
}

#[tokio::test]
async fn a_credentialed_framed_or_cross_site_subresource_request_is_refused_and_a_navigation_is_served()
 {
    let hosted = Hosted::start().await;
    let cookie = hosted.sign_in().await;
    let page = format!("/pages/{HOME}");
    let framed = hosted
        .get(&page)
        .cookie(&cookie)
        .header("Sec-Fetch-Dest", "iframe")
        .header("Sec-Fetch-Site", "same-origin")
        .header("Sec-Fetch-Mode", "navigate")
        .send()
        .await;
    assert_eq!(refused(&framed), (403, Some("request::framed".to_owned())));
    let subresource = hosted
        .get(&page)
        .cookie(&cookie)
        .header("Sec-Fetch-Dest", "image")
        .header("Sec-Fetch-Site", "cross-site")
        .header("Sec-Fetch-Mode", "no-cors")
        .send()
        .await;
    assert_eq!(
        refused(&subresource),
        (403, Some("request::cross_site".to_owned()))
    );
    let navigation = hosted
        .get(&page)
        .cookie(&cookie)
        .header("Sec-Fetch-Dest", "document")
        .header("Sec-Fetch-Site", "cross-site")
        .header("Sec-Fetch-Mode", "navigate")
        .send()
        .await;
    assert_eq!(navigation.status, 200, "{}", navigation.body);
}

#[tokio::test]
async fn a_hosted_server_reads_only_the_exact_session_cookie_and_skips_every_other_pair() {
    let hosted = Hosted::start().await;
    let cookie = hosted.sign_in().await;
    let secret = cookie.split_once('=').unwrap().1.to_owned();
    let page = format!("/pages/{HOME}");
    for impostor in [
        format!("flare_session={secret}"),
        format!("__HOST-flare_session={secret}"),
        format!("__Host-flare_session_x={secret}"),
    ] {
        let reply = hosted.get(&page).cookie(&impostor).send().await;
        assert_eq!(
            refused(&reply),
            (401, Some("auth::sign_in_needed".to_owned())),
            "{impostor}"
        );
    }
    let foreign = format!("theme=caf\u{e9}; {cookie}; other=\u{1F600}");
    let beside = hosted.get(&page).cookie(&foreign).send().await;
    assert_eq!(beside.status, 200, "{}", beside.body);
    assert_eq!(hosted.read_home(&cookie).await, 200);
}

#[tokio::test]
async fn a_session_value_that_is_not_a_token_or_a_repeated_session_is_refused_and_one_token_is_read()
 {
    let hosted = Hosted::start().await;
    let cookie = hosted.sign_in().await;
    let page = format!("/pages/{HOME}");
    for bad in [
        format!("{SESSION}=a\"b"),
        format!("{SESSION}=a\\b"),
        format!("{SESSION}=a,b"),
        format!("{SESSION}=caf\u{e9}"),
        format!("{SESSION}="),
        format!("{cookie}; {cookie}"),
    ] {
        let reply = hosted.get(&page).cookie(&bad).send().await;
        assert_eq!(
            refused(&reply),
            (401, Some("auth::bad_credential".to_owned())),
            "{bad}"
        );
    }
    assert_eq!(hosted.read_home(&cookie).await, 200);
}

#[tokio::test]
async fn a_cross_site_top_level_navigation_with_the_session_reads_the_page() {
    let hosted = Hosted::start().await;
    let cookie = hosted.sign_in().await;
    let reply = hosted
        .get(&format!("/pages/{HOME}"))
        .cookie(&cookie)
        .header("Sec-Fetch-Site", "cross-site")
        .header("Sec-Fetch-Mode", "navigate")
        .header("Sec-Fetch-Dest", "document")
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
}

#[tokio::test]
async fn a_hosted_server_answers_404_to_the_local_only_routes_and_reads_nothing_for_them() {
    let hosted = Hosted::start().await;
    hosted.seed_binding().await;
    let cookie = hosted.sign_in().await;
    let pages_before = hosted.switches.page_reads.load(Ordering::SeqCst);
    let records_before = hosted.switches.record_reads.load(Ordering::SeqCst);
    let binding = format!("/b/{BINDING}");
    let mut calls = vec![
        hosted.get(&binding).cookie(&cookie),
        hosted
            .request("POST", &binding)
            .cookie(&cookie)
            .same_origin()
            .form(&[]),
        hosted
            .request("POST", "/launch/codes?return_to=/flare/")
            .cookie(&cookie)
            .same_origin(),
    ];
    for path in [
        "/ready",
        "/launch?code=x",
        "/launch/probe?nonce=x",
        "/auth/oidc/finish?code=x",
    ] {
        calls.push(hosted.get(path).cookie(&cookie));
        calls.push(hosted.get(path));
    }
    for call in calls {
        let reply = call.clone().send().await;
        assert_eq!(
            refused(&reply),
            (404, Some("request::not_found".to_owned())),
            "{call:?}"
        );
    }
    let mut parallel = tokio::task::JoinSet::new();
    for at in 0..32 {
        let call = if at % 2 == 0 {
            hosted.get("/ready")
        } else {
            hosted.get(&binding).cookie(&cookie)
        };
        parallel.spawn(call.send());
    }
    while let Some(reply) = parallel.join_next().await {
        assert_eq!(reply.unwrap().status, 404);
    }
    assert_eq!(
        hosted.switches.page_reads.load(Ordering::SeqCst),
        pages_before
    );
    assert_eq!(
        hosted.switches.record_reads.load(Ordering::SeqCst),
        records_before
    );
    assert_eq!(hosted.read_home(&cookie).await, 200);
}

#[tokio::test]
async fn the_hosted_index_lists_pages_and_links_no_binding() {
    let hosted = Hosted::start_with(Admits::Domain, None).await;
    hosted.seed_binding().await;
    let cookie = hosted.sign_in_as("ada").await;
    let index = hosted.get("/flare/").cookie(&cookie).send().await;
    assert_eq!(index.status, 200, "{}", index.body);
    assert!(index.body.contains(&format!("href=\"/pages/{HOME}\"")));
    assert!(!index.body.contains("href=\"/b/"), "{}", index.body);
    let root = hosted.get("/").cookie(&cookie).send().await;
    assert_eq!(
        (root.status, root.header("location")),
        (303, Some("/flare/"))
    );
}

#[tokio::test]
async fn the_hosted_401_page_links_to_the_origins_sign_in_with_the_encoded_path() {
    let hosted = Hosted::start().await;
    let reply = hosted.get(&format!("/pages/{HOME}")).send().await;
    assert_eq!(
        refused(&reply),
        (401, Some("auth::sign_in_needed".to_owned()))
    );
    let link = format!(
        "{}/auth/oidc/start?return_to=%2Fpages%2F{HOME}",
        hosted.origin
    );
    assert!(reply.body.contains(&link), "{}", reply.body);
    assert!(!reply.body.contains("flare open"), "{}", reply.body);
    let signed_in = hosted.sign_in().await;
    assert_eq!(hosted.read_home(&signed_in).await, 200);
}

#[tokio::test]
async fn every_hosted_reply_refusals_included_carries_hsts_and_the_seven_headers() {
    let hosted = Hosted::start().await;
    let cookie = hosted.sign_in().await;
    let replies = [
        hosted.get("/up").send().await,
        hosted.get(&format!("/pages/{HOME}")).send().await,
        hosted.read_home_reply(&cookie).await,
        hosted.get("/ready").send().await,
        hosted.get("/up").host("evil.example").send().await,
        hosted
            .get(&format!("/auth/oidc/start?return_to=/pages/{HOME}"))
            .send()
            .await,
        hosted
            .get("/auth/oidc/callback?state=x&code=y")
            .send()
            .await,
    ];
    for reply in &replies {
        assert_eq!(
            reply.header("strict-transport-security"),
            Some(HSTS),
            "{}",
            reply.status
        );
        for (name, value) in FIXED {
            assert!(
                reply.header(name).is_some_and(|got| got.starts_with(value)),
                "{name} on {}",
                reply.status
            );
        }
    }
}

#[tokio::test]
async fn a_local_server_sends_no_hsts() {
    let (server, _fake, _clock) = oidc::oidc_server().await;
    let reply = server.get("/up").send().await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.header("strict-transport-security"), None);
}

#[tokio::test]
async fn a_hosted_server_keeps_serving_past_the_idle_period_and_stops_on_shutdown() {
    let mut hosted = Hosted::start_with(
        Admits::AllowList,
        Some(harness::limits(|limits| {
            limits.idle_exit = Duration::from_millis(200);
        })),
    )
    .await;
    assert_eq!(hosted.get("/up").send().await.status, 200);
    assert!(
        !hosted.returns_within(Duration::from_millis(800)).await,
        "the hosted server exited by itself"
    );
    assert_eq!(hosted.get("/up").send().await.status, 200);
    assert_eq!(hosted.stop().await, hangar_server::Stopped::Shutdown);
}

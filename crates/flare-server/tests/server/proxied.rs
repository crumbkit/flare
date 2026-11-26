//! The hosted site behind a single sign-on proxy: the proxy's signed
//! cookie opens a page, a refused cookie gets its own code and no
//! redirect, an unsigned identity header signs no one in, the `/auth/oidc`
//! routes are gone, and the share API takes the proxy's token as a bearer
//! beside flare's own tokens.

use crate::fake_proxy::{HOME, KeysReply, Sign};
use crate::proxied::{CLI_PORT, Proxied, SIGN_IN, SIGN_OUT, STATE};
use serde_json::json;

fn code(reply: &crate::harness::Reply) -> Option<String> {
    reply.code()
}

#[tokio::test]
async fn the_proxys_signed_cookie_opens_the_reading_page() {
    let site = Proxied::start().await;
    let token = site.fake.token(Sign::Rs256);
    let reply = site
        .get(&format!("/pages/{HOME}"))
        .cookie(&Proxied::cookie(&token))
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
}

#[tokio::test]
async fn a_reader_with_no_cookie_gets_a_401_that_links_to_the_proxys_sign_in() {
    let site = Proxied::start().await;
    let reply = site.get(&format!("/pages/{HOME}")).send().await;
    assert_eq!(reply.status, 401);
    assert_eq!(code(&reply).as_deref(), Some("auth::sign_in_needed"));
    assert!(reply.header("location").is_none());
    assert!(reply.body.contains(SIGN_IN), "{}", reply.body);
    assert!(!reply.body.contains("/auth/oidc/start"), "{}", reply.body);
}

#[tokio::test]
async fn a_reader_with_no_cookie_and_no_sign_in_address_gets_a_401_without_a_link() {
    let site = Proxied::start_with(|config| config).await;
    let reply = site.get(&format!("/pages/{HOME}")).send().await;
    assert_eq!(reply.status, 401);
    assert_eq!(code(&reply).as_deref(), Some("auth::sign_in_needed"));
    assert!(!reply.body.contains("href=\"https://"), "{}", reply.body);
    assert!(!reply.body.contains("/auth/oidc/start"), "{}", reply.body);
}

#[tokio::test]
async fn a_refused_cookie_gets_proxy_refused_with_no_redirect_or_sign_in_link() {
    let site = Proxied::start().await;
    let other = site.fake.token_with(Sign::Rs256, |claims| {
        claims.insert("aud".into(), json!("another-app"));
    });
    for token in [other, site.fake.token(Sign::AlgNone)] {
        let reply = site
            .get(&format!("/pages/{HOME}"))
            .cookie(&Proxied::cookie(&token))
            .send()
            .await;
        assert_eq!(reply.status, 401);
        assert_eq!(code(&reply).as_deref(), Some("auth::proxy_refused"));
        assert!(reply.header("location").is_none());
        assert!(!reply.body.contains(SIGN_IN), "{}", reply.body);
    }
}

#[tokio::test]
async fn an_address_the_rule_does_not_admit_gets_403_with_the_sign_out_link() {
    let site = Proxied::start().await;
    let stranger = site.fake.token_with(Sign::Rs256, |claims| {
        claims.insert("email".into(), json!("eve@example.com"));
    });
    let reply = site
        .get(&format!("/pages/{HOME}"))
        .cookie(&Proxied::cookie(&stranger))
        .send()
        .await;
    assert_eq!(reply.status, 403);
    assert_eq!(code(&reply).as_deref(), Some("auth::not_admitted"));
    assert!(reply.body.contains(SIGN_OUT), "{}", reply.body);
}

#[tokio::test]
async fn an_unsigned_identity_header_signs_no_one_in() {
    let site = Proxied::start().await;
    let reply = site
        .get(&format!("/pages/{HOME}"))
        .header("X-Forwarded-Email", crate::fake_proxy::EMAIL)
        .header("Remote-User", crate::fake_proxy::EMAIL)
        .send()
        .await;
    assert_eq!(reply.status, 401);
    assert_eq!(code(&reply).as_deref(), Some("auth::sign_in_needed"));
}

#[tokio::test]
async fn the_token_in_flares_own_session_cookie_is_not_read() {
    let site = Proxied::start().await;
    let token = site.fake.token(Sign::Rs256);
    let reply = site
        .get(&format!("/pages/{HOME}"))
        .cookie(&format!("__Host-flare_session={token}"))
        .send()
        .await;
    assert_eq!(reply.status, 401);
    assert_eq!(code(&reply).as_deref(), Some("auth::sign_in_needed"));
}

#[tokio::test]
async fn the_openid_connect_routes_answer_404() {
    let site = Proxied::start().await;
    for path in [
        format!("/auth/oidc/start?return_to=/pages/{HOME}"),
        "/auth/oidc/callback?state=x&code=y".to_owned(),
    ] {
        let reply = site.get(&path).send().await;
        assert_eq!(reply.status, 404, "{path}");
    }
}

#[tokio::test]
async fn a_key_set_that_cannot_be_fetched_for_an_unknown_key_gives_503() {
    let site = Proxied::start().await;
    site.fake.keys_reply(KeysReply::Down);
    let reply = site
        .get(&format!("/pages/{HOME}"))
        .cookie(&Proxied::cookie(&site.fake.token(Sign::UnpublishedKey)))
        .send()
        .await;
    assert_eq!(reply.status, 503);
    assert_eq!(code(&reply).as_deref(), Some("auth::unavailable"));
}

#[tokio::test]
async fn a_script_on_the_site_reads_activity_with_the_proxy_cookie_only_from_the_same_origin() {
    let site = Proxied::start().await;
    let token = site.fake.token(Sign::Rs256);
    let flare = site.share(&token).await;
    let cookie = Proxied::cookie(&token);
    let path = format!("/api/activity?flare={flare}");
    let cross = site.get(&path).cookie(&cookie).send().await;
    assert_eq!(cross.status, 403, "{}", cross.body);
    assert_eq!(code(&cross).as_deref(), Some("request::origin"));
    let same = site.get(&path).cookie(&cookie).same_origin().send().await;
    assert_eq!(same.status, 200, "{}", same.body);
}

#[tokio::test]
async fn the_share_api_takes_the_proxy_token_as_a_bearer_and_refuses_a_forged_one() {
    let site = Proxied::start().await;
    let token = site.fake.token(Sign::Es256);
    let listed = site
        .get("/api/shares?scope=mine")
        .bearer(&token)
        .send()
        .await;
    assert_eq!(listed.status, 200, "{}", listed.body);
    let forged = site
        .get("/api/shares?scope=mine")
        .bearer(&site.fake.token(Sign::BadSignature))
        .send()
        .await;
    assert_eq!(forged.status, 401, "{}", forged.body);
    assert_eq!(
        code(&forged).as_deref(),
        Some("flare::tokens::sign_in_needed")
    );
}

#[tokio::test]
async fn flare_login_gets_a_flare_token_through_the_proxy_cookie_and_the_api_accepts_it() {
    let site = Proxied::start().await;
    let proxy_token = site.fake.token(Sign::Rs256);
    let flare_token = site.flare_token(&proxy_token).await;
    assert!(flare_token.starts_with("flare_"), "a flare token");
    let listed = site
        .get("/api/shares?scope=mine")
        .bearer(&flare_token)
        .send()
        .await;
    assert_eq!(listed.status, 200, "{}", listed.body);
}

#[tokio::test]
async fn cli_login_without_a_cookie_sends_the_browser_to_the_proxys_sign_in() {
    let site = Proxied::start().await;
    let reply = site
        .get(&format!("/cli/login?port={CLI_PORT}&state={STATE}"))
        .send()
        .await;
    assert_eq!(reply.status, 303, "{}", reply.body);
    assert_eq!(reply.header("location"), Some(SIGN_IN));
}

#[tokio::test]
async fn cli_login_with_a_refused_or_unadmitted_cookie_answers_like_a_page_and_does_not_redirect() {
    let site = Proxied::start().await;
    let other = site.fake.token_with(Sign::Rs256, |claims| {
        claims.insert("aud".into(), json!("another-app"));
    });
    let stranger = site.fake.token_with(Sign::Rs256, |claims| {
        claims.insert("email".into(), json!("eve@example.com"));
    });
    let path = format!("/cli/login?port={CLI_PORT}&state={STATE}");
    for (token, status, expected) in [
        (other, 401, "auth::proxy_refused"),
        (stranger, 403, "auth::not_admitted"),
    ] {
        let reply = site
            .get(&path)
            .cookie(&Proxied::cookie(&token))
            .send()
            .await;
        assert_eq!(reply.status, status, "{}", reply.body);
        assert_eq!(code(&reply).as_deref(), Some(expected));
        assert!(reply.header("location").is_none());
    }
    // The admitted twin: the proxy's valid cookie gets the redirect to the
    // CLI's loopback listener.
    let token = site.fake.token(Sign::Rs256);
    let reply = site
        .get(&path)
        .cookie(&Proxied::cookie(&token))
        .send()
        .await;
    assert_eq!(reply.status, 303, "{}", reply.body);
    let location = reply.header("location").unwrap_or_default();
    assert!(
        location.starts_with(&format!("http://127.0.0.1:{CLI_PORT}/")),
        "{location}"
    );
}

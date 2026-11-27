//! The share API on the hosted site: CLI login and tokens, sharing with
//! named people, responses with `--re`, and activity since a cursor. Each
//! refusal test also shows the admitted case.

use crate::api::{Api, CLI_PORT, STATE, share_body};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::time::Duration;

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

fn ids(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["share"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn a_person_logs_in_from_the_cli_and_their_token_lists_their_shares() {
    let api = Api::start().await;
    let issued = api.issued("alice", "studio").await;
    assert_eq!(issued["owner"], "alice@example.com");
    assert_eq!(issued["label"], "studio");
    let token = issued["token"].as_str().unwrap();
    assert!(token.starts_with("flare_"));
    let (status, list) = api.read("GET", "/api/shares?scope=mine", Some(token)).await;
    assert_eq!(status, 200, "{list}");
    assert_eq!(list["items"], json!([]));
}

#[tokio::test]
async fn a_browser_with_no_session_is_sent_to_sign_in_and_returns_to_the_cli_login() {
    let api = Api::start().await;
    let reply = api.cli_login(None).await;
    assert_eq!(reply.status, 303);
    let location = reply.header("location").unwrap();
    assert!(
        location.starts_with("/auth/oidc/start?return_to="),
        "{location}"
    );
    assert!(location.contains("%2Fcli%2Flogin"), "{location}");
    let cookie = api.session("alice").await;
    let signed_in = api.cli_login(Some(&cookie)).await;
    assert_eq!(signed_in.status, 303);
    let target = signed_in.header("location").unwrap();
    assert!(target.starts_with(&format!("http://127.0.0.1:{CLI_PORT}/callback?")));
    assert!(target.contains(STATE));
}

#[tokio::test]
async fn a_cli_login_with_a_malformed_port_or_state_is_refused() {
    let api = Api::start().await;
    let cookie = api.session("alice").await;
    for query in [
        "port=0&state=teststate0123456789abcdef",
        "port=80x&state=s",
        "port=4000&state=short",
    ] {
        let reply = api
            .hosted
            .get(&format!("/cli/login?{query}"))
            .cookie(&cookie)
            .send()
            .await;
        assert_eq!(reply.status, 400, "{query}");
        assert_eq!(reply.code().as_deref(), Some("flare::requests::bad_query"));
    }
    assert_eq!(api.cli_login(Some(&cookie)).await.status, 303);
}

#[tokio::test]
async fn a_login_code_buys_one_token_and_a_reused_or_unknown_code_is_refused() {
    let api = Api::start().await;
    let code_text = api.code_for("alice").await;
    let (first, _) = api.exchange(&code_text, "studio").await;
    assert_eq!(first, 201);
    let (again, body) = api.exchange(&code_text, "studio").await;
    assert_eq!((again, code(&body)), (403, "flare::tokens::code_refused"));
    let (unknown, body) = api.exchange(&"0".repeat(64), "studio").await;
    assert_eq!((unknown, code(&body)), (403, "flare::tokens::code_refused"));
    assert_eq!(
        body["recovery"]["call"],
        format!("flare login {}", api.origin())
    );
}

#[tokio::test]
async fn a_login_code_older_than_60_seconds_is_refused_and_a_fresh_one_works() {
    let api = Api::start().await;
    let late = api.code_for("alice").await;
    api.hosted.time.advance(Duration::from_secs(61));
    let (status, body) = api.exchange(&late, "studio").await;
    assert_eq!((status, code(&body)), (403, "flare::tokens::code_refused"));
    let fresh = api.code_for("alice").await;
    assert_eq!(api.exchange(&fresh, "studio").await.0, 201);
}

#[tokio::test]
async fn an_unknown_revoked_or_expired_token_gets_401_with_flare_login_as_its_recovery() {
    let api = Api::start().await;
    let issued = api.issued("alice", "studio").await;
    let token = issued["token"].as_str().unwrap().to_owned();
    let path = "/api/shares?scope=mine";
    assert_eq!(api.read("GET", path, Some(&token)).await.0, 200);
    let forged = format!("{}{}", &token[..token.len() - 2], "AA");
    for bad in [forged.as_str(), "flare_nonsense", "something-else"] {
        let (status, body) = api.read("GET", path, Some(bad)).await;
        assert_eq!(
            (status, code(&body)),
            (401, "flare::tokens::sign_in_needed"),
            "{bad}"
        );
        assert_eq!(
            body["recovery"]["call"],
            format!("flare login {}", api.origin())
        );
    }
    let (missing, _) = api.read("GET", path, None).await;
    assert_eq!(missing, 401);
    let other = api.token_for("bob").await;
    api.hosted.time.advance(91 * DAY);
    let (expired, body) = api.read("GET", path, Some(&other)).await;
    assert_eq!(
        (expired, code(&body)),
        (401, "flare::tokens::sign_in_needed")
    );
    let id = issued["id"].as_str().unwrap();
    let fresh = api.token_for("alice").await;
    assert_eq!(api.read("GET", path, Some(&fresh)).await.0, 200);
    let (revoked, _) = api
        .read("DELETE", &format!("/api/tokens/{id}"), Some(&fresh))
        .await;
    assert_eq!(revoked, 204);
}

#[tokio::test]
async fn logging_out_revokes_the_token_at_once() {
    let api = Api::start().await;
    let issued = api.issued("alice", "studio").await;
    let token = issued["token"].as_str().unwrap();
    let id = issued["id"].as_str().unwrap();
    let (status, _) = api
        .read("DELETE", &format!("/api/tokens/{id}"), Some(token))
        .await;
    assert_eq!(status, 204);
    let (after, body) = api.read("GET", "/api/shares?scope=mine", Some(token)).await;
    assert_eq!((after, code(&body)), (401, "flare::tokens::sign_in_needed"));
}

#[tokio::test]
async fn a_person_cannot_revoke_another_persons_token_and_can_revoke_their_own() {
    let api = Api::start().await;
    let alice = api.issued("alice", "studio").await;
    let bob = api.token_for("bob").await;
    let path = format!("/api/tokens/{}", alice["id"].as_str().unwrap());
    let (status, body) = api.read("DELETE", &path, Some(&bob)).await;
    assert_eq!((status, code(&body)), (404, "flare::tokens::not_found"));
    let own = alice["token"].as_str().unwrap();
    assert_eq!(api.read("DELETE", &path, Some(own)).await.0, 204);
}

#[tokio::test]
async fn a_second_login_with_the_same_label_revokes_the_first_token() {
    let api = Api::start().await;
    let first = api.issued("alice", "studio").await;
    let other_label = api.issued("alice", "laptop").await;
    let second = api.issued("alice", "studio").await;
    let path = "/api/shares?scope=mine";
    let token = |issued: &Value| issued["token"].as_str().unwrap().to_owned();
    assert_eq!(api.read("GET", path, Some(&token(&first))).await.0, 401);
    assert_eq!(
        api.read("GET", path, Some(&token(&other_label))).await.0,
        200
    );
    assert_eq!(api.read("GET", path, Some(&token(&second))).await.0, 200);
}

#[tokio::test]
async fn the_api_ignores_a_browser_session_so_a_cross_site_form_cannot_share() {
    let api = Api::start().await;
    let cookie = api.session("alice").await;
    let reply = api
        .call("POST", "/api/shares", None)
        .cookie(&cookie)
        .body(
            "application/json",
            share_body("Notes", &[], &[]).to_string().into_bytes(),
        )
        .send()
        .await;
    assert_eq!(reply.status, 401);
    let token = api.token_for("alice").await;
    api.share(&token, "Notes", &[], &[]).await;
}

#[tokio::test]
async fn a_creator_shares_markdown_and_each_recipient_lists_and_opens_it() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let (status, shared) = api
        .json(
            "POST",
            "/api/shares",
            Some(&alice),
            share_body(
                "Weather holds",
                &["bob@example.com", "carol@example.com"],
                &[],
            ),
        )
        .await;
    assert_eq!(status, 201, "{shared}");
    assert_eq!(
        shared["granted"],
        json!(["bob@example.com", "carol@example.com"])
    );
    let summary = &shared["share"];
    assert_eq!(summary["title"], "Weather holds");
    assert_eq!(summary["format"], "markdown");
    assert_eq!(summary["owner"], "alice@example.com");
    assert_eq!(summary["role"], "owner");
    assert_eq!(summary["version"], 1);
    let id = summary["id"].as_str().unwrap();
    let bob = api.token_for("bob").await;
    let (_, list) = api
        .read("GET", "/api/shares?scope=with-me", Some(&bob))
        .await;
    assert_eq!(list["items"][0]["id"], id);
    assert_eq!(list["items"][0]["role"], "reader");
    let (status, opened) = api
        .read("GET", &format!("/api/shares/{id}"), Some(&bob))
        .await;
    assert_eq!(status, 200);
    assert_eq!(opened["content"], "# Weather holds\n");
}

#[tokio::test]
async fn a_share_list_obeys_its_limit_and_says_more() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    for title in ["One", "Two", "Three"] {
        api.share(&alice, title, &[], &[]).await;
    }
    let (_, two) = api
        .read("GET", "/api/shares?scope=mine&limit=2", Some(&alice))
        .await;
    assert_eq!(two["items"].as_array().unwrap().len(), 2);
    assert_eq!(two["more"], true);
    let (_, all) = api
        .read("GET", "/api/shares?scope=mine&limit=500", Some(&alice))
        .await;
    assert_eq!(all["items"].as_array().unwrap().len(), 3);
    assert_eq!(all["more"], false);
    let (status, body) = api
        .read("GET", "/api/shares?scope=everyone", Some(&alice))
        .await;
    assert_eq!((status, code(&body)), (400, "flare::requests::bad_query"));
}

/// The ids of a share list's items.
fn listed(list: &Value) -> Vec<String> {
    list["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn a_share_list_names_next_on_the_first_of_two_pages_and_none_on_the_last() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    for title in ["One", "Two", "Three"] {
        api.share(&alice, title, &["bob@example.com"], &[]).await;
    }
    let (_, whole) = api
        .read("GET", "/api/shares?scope=with-me&limit=500", Some(&bob))
        .await;
    assert_eq!(whole["more"], false);
    assert!(whole.get("next").is_none(), "{whole}");
    let (status, first) = api
        .read("GET", "/api/shares?scope=with-me&limit=2", Some(&bob))
        .await;
    assert_eq!(status, 200, "{first}");
    assert_eq!(first["more"], true);
    let next = first["next"].as_str().unwrap().to_owned();
    assert!(next.starts_with("fs1."), "{next}");
    let (status, second) = api
        .read(
            "GET",
            &format!("/api/shares?scope=with-me&limit=2&cursor={next}"),
            Some(&bob),
        )
        .await;
    assert_eq!(status, 200, "{second}");
    assert_eq!(second["more"], false);
    assert!(second.get("next").is_none(), "{second}");
    let mut walked = listed(&first);
    walked.extend(listed(&second));
    assert_eq!(walked, listed(&whole));

    let (_, mine) = api
        .read("GET", "/api/shares?scope=mine&limit=2", Some(&alice))
        .await;
    let next = mine["next"].as_str().unwrap();
    let (_, rest) = api
        .read(
            "GET",
            &format!("/api/shares?scope=mine&limit=2&cursor={next}"),
            Some(&alice),
        )
        .await;
    assert_eq!(listed(&rest).len(), 1, "{rest}");
}

#[tokio::test]
async fn a_cursor_that_is_malformed_of_activity_or_of_the_other_scope_is_bad_cursor_and_two_are_a_bad_query()
 {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    for title in ["One", "Two"] {
        api.share(&alice, title, &[], &[]).await;
    }
    let (_, first) = api
        .read("GET", "/api/shares?scope=mine&limit=1", Some(&alice))
        .await;
    let next = first["next"].as_str().unwrap().to_owned();
    for path in [
        "/api/shares?scope=mine&cursor=nonsense".to_owned(),
        "/api/shares?scope=mine&cursor=fc1.AAAAAAAAAAA".to_owned(),
        format!("/api/shares?scope=with-me&cursor={next}"),
    ] {
        let (status, body) = api.read("GET", &path, Some(&alice)).await;
        assert_eq!(
            (status, code(&body)),
            (400, "flare::shares::bad_cursor"),
            "{path}"
        );
    }
    let (status, body) = api
        .read(
            "GET",
            &format!("/api/shares?scope=mine&cursor={next}&cursor={next}"),
            Some(&alice),
        )
        .await;
    assert_eq!((status, code(&body)), (400, "flare::requests::bad_query"));
    let (status, page) = api
        .read(
            "GET",
            &format!("/api/shares?scope=mine&cursor={next}"),
            Some(&alice),
        )
        .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(listed(&page).len(), 1);
}

#[tokio::test]
async fn a_cursor_from_another_caller_lists_only_the_callers_own_shares() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    let carol = api.token_for("carol").await;
    // Carol's one grant is older than every grant of Bob's, so it sorts
    // after any key Bob's list gives, and her page after his key holds it.
    let theirs = api
        .share(&alice, "To carol", &["carol@example.com"], &[])
        .await;
    for title in ["One", "Two", "Three"] {
        api.hosted.time.advance(Duration::from_secs(10));
        api.share(&alice, title, &["bob@example.com"], &[]).await;
    }
    api.hosted.time.advance(Duration::from_secs(10));
    api.share(&carol, "Carol's", &["bob@example.com"], &[])
        .await;
    let (_, bobs) = api
        .read("GET", "/api/shares?scope=with-me&limit=1", Some(&bob))
        .await;
    let next = bobs["next"].as_str().unwrap();
    let (status, page) = api
        .read(
            "GET",
            &format!("/api/shares?scope=with-me&cursor={next}"),
            Some(&carol),
        )
        .await;
    assert_eq!(status, 200, "{page}");
    assert_eq!(listed(&page), [theirs], "{page}");
    assert!(page.get("next").is_none(), "{page}");
}

#[tokio::test]
async fn a_share_list_whose_share_store_fails_answers_503_with_no_items_and_answers_once_it_recovers()
 {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    api.share(&alice, "One", &["bob@example.com"], &[]).await;
    api.faults.shares.store(true, Ordering::SeqCst);
    for (path, token) in [
        ("/api/shares?scope=with-me&limit=1", &bob),
        ("/api/shares?scope=mine&limit=1", &alice),
    ] {
        let (status, body) = api.read("GET", path, Some(token)).await;
        assert_eq!((status, code(&body)), (503, "flare::shares::unavailable"));
        assert!(body.get("items").is_none(), "{body}");
    }
    api.faults.shares.store(false, Ordering::SeqCst);
    let (status, page) = api
        .read("GET", "/api/shares?scope=with-me&limit=1", Some(&bob))
        .await;
    assert_eq!((status, listed(&page).len()), (200, 1));
}

#[tokio::test]
async fn a_non_creator_gets_not_a_creator_for_a_share_and_a_response_and_a_creator_succeeds() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let flare = api.share(&alice, "Plan", &["dan@example.com"], &[]).await;
    let dan = api.token_for("dan").await;
    for re in [&[][..], &[flare.as_str()][..]] {
        let (status, body) = api
            .json(
                "POST",
                "/api/shares",
                Some(&dan),
                share_body("Reply", &[], re),
            )
            .await;
        assert_eq!((status, code(&body)), (403, "flare::shares::not_a_creator"));
        assert!(
            body["recovery"]["text"]
                .as_str()
                .unwrap()
                .contains("operator")
        );
    }
    let bob = api.token_for("bob").await;
    let (_, _) = api
        .json(
            "POST",
            &format!("/api/grants/{flare}"),
            Some(&alice),
            json!({"with": ["bob@example.com"]}),
        )
        .await;
    api.share(&bob, "Reply", &[], &[&flare]).await;
}

#[tokio::test]
async fn html_sent_as_json_is_refused_as_needing_a_form_and_markdown_is_served() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let html = json!({"title": "Page", "format": "html", "content": "<p>x</p>"});
    let (status, body) = api.json("POST", "/api/shares", Some(&alice), html).await;
    assert_eq!((status, code(&body)), (415, "flare::html::needs_form"));
    api.share(&alice, "Page", &[], &[]).await;
}

#[tokio::test]
async fn a_name_in_with_is_refused_with_the_address_form_and_an_address_is_accepted() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let (status, body) = api
        .json(
            "POST",
            "/api/shares",
            Some(&alice),
            share_body("Notes", &["ana"], &[]),
        )
        .await;
    assert_eq!(
        (status, code(&body)),
        (400, "flare::recipients::not_an_address")
    );
    assert!(
        body["recovery"]["text"]
            .as_str()
            .unwrap()
            .contains("ana@example.org")
    );
    let (_, mine) = api
        .read("GET", "/api/shares?scope=mine", Some(&alice))
        .await;
    assert_eq!(
        mine["items"],
        json!([]),
        "a refused recipient writes nothing"
    );
    api.share(&alice, "Notes", &["ana@example.org"], &[]).await;
}

#[tokio::test]
async fn a_missing_flare_and_a_refused_flare_give_the_same_body_to_re_activity_and_open() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let carol = api.token_for("carol").await;
    let refused = api.share(&alice, "Private", &[], &[]).await;
    let missing = "0123456789abcdef0123456789abcdef";
    let mut bodies = Vec::new();
    for flare in [refused.as_str(), missing] {
        let re = api
            .json(
                "POST",
                "/api/shares",
                Some(&carol),
                share_body("Reply", &[], &[flare]),
            )
            .await;
        let activity = api
            .read("GET", &format!("/api/activity?flare={flare}"), Some(&carol))
            .await;
        let open = api
            .read("GET", &format!("/api/shares/{flare}"), Some(&carol))
            .await;
        bodies.push([re, activity, open]);
    }
    assert_eq!(bodies[0], bodies[1]);
    for (status, body) in &bodies[0] {
        assert_eq!((*status, code(body)), (404, "flare::shares::not_found"));
    }
    let (status, _) = api
        .read("GET", &format!("/api/shares/{refused}"), Some(&alice))
        .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn activity_omits_a_response_the_caller_cannot_open_and_shows_it_once_granted() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    let carol = api.token_for("carol").await;
    let flare = api
        .share(
            &alice,
            "Plan",
            &["bob@example.com", "carol@example.com"],
            &[],
        )
        .await;
    let response = api
        .share(&bob, "Counter-plan", &["alice@example.com"], &[&flare])
        .await;
    let path = format!("/api/activity?flare={flare}");
    let (status, hidden) = api.read("GET", &path, Some(&carol)).await;
    assert_eq!(status, 200);
    assert_eq!(hidden["items"], json!([]));
    assert!(hidden.get("hidden").is_none() && hidden.get("count").is_none());
    let (_, _) = api
        .json(
            "POST",
            &format!("/api/grants/{response}"),
            Some(&bob),
            json!({"with": ["carol@example.com"]}),
        )
        .await;
    let (_, shown) = api.read("GET", &path, Some(&carol)).await;
    let item = &shown["items"][0];
    assert_eq!(item["kind"], "response");
    assert_eq!(item["flare"], flare.as_str());
    assert_eq!(item["share"], response.as_str());
    assert_eq!(item["title"], "Counter-plan");
    assert_eq!(item["by"], "bob@example.com");
    assert_eq!(item["version"], 1);
    assert!(item["at"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn a_cursor_returns_only_newer_responses_and_a_malformed_cursor_is_refused() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    let flare = api.share(&alice, "Plan", &["bob@example.com"], &[]).await;
    let first = api
        .share(&bob, "First", &["alice@example.com"], &[&flare])
        .await;
    let path = format!("/api/activity?flare={flare}");
    let (_, page) = api.read("GET", &path, Some(&alice)).await;
    assert_eq!(ids(&page), vec![first]);
    let cursor = page["cursor"].as_str().unwrap().to_owned();
    assert!(cursor.starts_with("fc1."));
    let second = api
        .share(&bob, "Second", &["alice@example.com"], &[&flare])
        .await;
    let (_, newer) = api
        .read("GET", &format!("{path}&since={cursor}"), Some(&alice))
        .await;
    assert_eq!(ids(&newer), vec![second]);
    let (status, body) = api
        .read("GET", &format!("{path}&since=fc9.nope"), Some(&alice))
        .await;
    assert_eq!((status, code(&body)), (400, "flare::activity::bad_cursor"));
}

#[tokio::test]
async fn a_cursor_from_one_flare_opens_nothing_on_a_flare_the_caller_cannot_open() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    let shared = api.share(&alice, "Plan", &["bob@example.com"], &[]).await;
    let private = api.share(&alice, "Private", &[], &[]).await;
    let (_, page) = api
        .read("GET", &format!("/api/activity?flare={shared}"), Some(&bob))
        .await;
    let cursor = page["cursor"].as_str().unwrap();
    let path = |flare: &str| format!("/api/activity?flare={flare}&since={cursor}");
    let (status, body) = api.read("GET", &path(&private), Some(&bob)).await;
    assert_eq!((status, code(&body)), (404, "flare::shares::not_found"));
    assert_eq!(api.read("GET", &path(&shared), Some(&bob)).await.0, 200);
}

#[tokio::test]
async fn activity_names_at_most_16_flares() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let flare = api.share(&alice, "Plan", &[], &[]).await;
    let many: Vec<String> = (0..17).map(|n| format!("flare={n:032x}")).collect();
    let (status, body) = api
        .read(
            "GET",
            &format!("/api/activity?{}", many.join("&")),
            Some(&alice),
        )
        .await;
    assert_eq!(
        (status, code(&body)),
        (400, "flare::shares::too_many_flares")
    );
    let (status, _) = api
        .read("GET", &format!("/api/activity?flare={flare}"), Some(&alice))
        .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn a_reader_cannot_add_a_flare_a_share_responds_to_and_the_owner_can() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    let flare = api.share(&alice, "Plan", &["bob@example.com"], &[]).await;
    let other = api.share(&alice, "Other", &["bob@example.com"], &[]).await;
    let body = json!({"re": [flare]});
    let (status, refused) = api
        .json(
            "POST",
            &format!("/api/grants/{other}"),
            Some(&bob),
            body.clone(),
        )
        .await;
    assert_eq!((status, code(&refused)), (403, "flare::shares::forbidden"));
    let (status, granted) = api
        .json("POST", &format!("/api/grants/{other}"), Some(&alice), body)
        .await;
    assert_eq!(status, 200, "{granted}");
    assert_eq!(granted["responds_to"], json!([flare]));
}

#[tokio::test]
async fn a_stranger_granting_a_flare_gets_not_found_and_the_owner_grants() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let carol = api.token_for("carol").await;
    let flare = api.share(&alice, "Plan", &[], &[]).await;
    let body = json!({"with": ["carol@example.com"]});
    let (status, refused) = api
        .json(
            "POST",
            &format!("/api/grants/{flare}"),
            Some(&carol),
            body.clone(),
        )
        .await;
    assert_eq!((status, code(&refused)), (404, "flare::shares::not_found"));
    let (status, granted) = api
        .json("POST", &format!("/api/grants/{flare}"), Some(&alice), body)
        .await;
    assert_eq!(status, 200);
    assert_eq!(granted["granted"], json!(["carol@example.com"]));
}

#[tokio::test]
async fn a_share_whose_grants_pass_the_limit_names_the_share_and_a_grant_past_it_is_too_many() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let people: Vec<String> = (0..=256).map(|n| format!("p{n}@example.com")).collect();
    let with: Vec<&str> = people.iter().map(String::as_str).collect();
    let (status, body) = api
        .json(
            "POST",
            "/api/shares",
            Some(&alice),
            share_body("Big", &with, &[]),
        )
        .await;
    assert_eq!((status, code(&body)), (409, "flare::shares::partly_shared"));
    let share = body["share"].as_str().unwrap();
    let (status, more) = api
        .json(
            "POST",
            &format!("/api/grants/{share}"),
            Some(&alice),
            json!({"with": ["late@example.com"]}),
        )
        .await;
    assert_eq!((status, code(&more)), (409, "flare::grants::too_many"));
    let (status, _) = api
        .json(
            "POST",
            "/api/shares",
            Some(&alice),
            share_body("Big", &with[..256], &[]),
        )
        .await;
    assert_eq!(status, 201);
}

#[tokio::test]
async fn a_share_over_256_kib_is_too_large_and_one_at_256_kib_is_served() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let body =
        |size: usize| json!({"title": "Big", "format": "markdown", "content": "x".repeat(size)});
    let (status, refused) = api
        .json("POST", "/api/shares", Some(&alice), body(262_145))
        .await;
    assert_eq!((status, code(&refused)), (413, "flare::shares::too_large"));
    let (status, _) = api
        .json("POST", "/api/shares", Some(&alice), body(262_144))
        .await;
    assert_eq!(status, 201);
}

#[tokio::test]
async fn an_empty_title_is_invalid_and_a_body_that_is_not_json_is_a_bad_body() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let (status, body) = api
        .json(
            "POST",
            "/api/shares",
            Some(&alice),
            share_body("", &[], &[]),
        )
        .await;
    assert_eq!((status, code(&body)), (400, "flare::shares::invalid"));
    let reply = api
        .call("POST", "/api/shares", Some(&alice))
        .body("application/json", b"{not json".to_vec())
        .send()
        .await;
    assert_eq!(reply.status, 400);
    assert_eq!(reply.code().as_deref(), Some("flare::requests::bad_body"));
    api.share(&alice, "Titled", &[], &[]).await;
}

#[tokio::test]
async fn each_store_that_fails_answers_its_unavailable_code_and_answers_again_once_it_recovers() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let flare = api.share(&alice, "Plan", &[], &[]).await;
    let activity = format!("/api/activity?flare={flare}");
    let cases = [
        (&api.faults.shares, "flare::shares::unavailable"),
        (&api.faults.responses, "flare::activity::unavailable"),
    ];
    for (switch, expected) in cases {
        switch.store(true, Ordering::SeqCst);
        let (status, body) = api.read("GET", &activity, Some(&alice)).await;
        assert_eq!((status, code(&body)), (503, expected));
        switch.store(false, Ordering::SeqCst);
        assert_eq!(api.read("GET", &activity, Some(&alice)).await.0, 200);
    }
    api.faults.recipients.store(true, Ordering::SeqCst);
    let (status, body) = api
        .json(
            "POST",
            "/api/shares",
            Some(&alice),
            share_body("To", &["bob@example.com"], &[]),
        )
        .await;
    assert_eq!(
        (status, code(&body)),
        (503, "flare::recipients::unavailable")
    );
    api.faults.recipients.store(false, Ordering::SeqCst);
    api.share(&alice, "To", &["bob@example.com"], &[]).await;
    api.faults.tokens.store(true, Ordering::SeqCst);
    let (status, body) = api
        .read("GET", "/api/shares?scope=mine", Some(&alice))
        .await;
    assert_eq!((status, code(&body)), (503, "flare::tokens::unavailable"));
    api.faults.tokens.store(false, Ordering::SeqCst);
    assert_eq!(
        api.read("GET", "/api/shares?scope=mine", Some(&alice))
            .await
            .0,
        200
    );
}

#[tokio::test]
async fn a_creators_rule_that_cannot_answer_refuses_a_share_and_a_team_with_503_and_writes_neither()
{
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    api.faults.creators.store(true, Ordering::SeqCst);
    let (status, body) = api
        .json(
            "POST",
            "/api/shares",
            Some(&alice),
            share_body("Plan", &[], &[]),
        )
        .await;
    assert_eq!((status, code(&body)), (503, "flare::creators::unavailable"));
    let (status, body) = api
        .json(
            "POST",
            "/api/teams",
            Some(&alice),
            json!({ "name": "design" }),
        )
        .await;
    assert_eq!((status, code(&body)), (503, "flare::teams::unavailable"));
    let (status, mine) = api
        .read("GET", "/api/shares?scope=mine", Some(&alice))
        .await;
    assert_eq!(status, 200);
    assert_eq!(mine["items"], json!([]), "{mine}");
    api.faults.creators.store(false, Ordering::SeqCst);
    api.share(&alice, "Plan", &[], &[]).await;
}

#[tokio::test]
async fn the_api_answers_404_on_a_hosted_site_without_the_share_api() {
    let hosted = crate::hosted::Hosted::start().await;
    let reply = hosted
        .get("/api/shares?scope=mine")
        .bearer("flare_x")
        .send()
        .await;
    assert_eq!(reply.status, 404);
    let api = Api::start().await;
    let token = api.token_for("alice").await;
    assert_eq!(
        api.read("GET", "/api/shares?scope=mine", Some(&token))
            .await
            .0,
        200
    );
}

#[tokio::test]
async fn a_response_from_someone_else_opens_for_the_flares_owner_without_naming_them() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let flare = api.share(&alice, "Plan", &["bob@example.com"], &[]).await;
    let bob = api.token_for("bob").await;
    let response = api.share(&bob, "Answer", &[], &[&flare]).await;
    let path = format!("/api/shares/{response}");
    let (status, body) = api.read("GET", &path, Some(&alice)).await;
    assert_eq!(status, 200, "{body}");
}

#[tokio::test]
async fn a_repeated_grant_writes_the_activity_entries_that_a_failed_index_write_left_out() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    let carol = api.token_for("carol").await;
    let flare = api
        .share(
            &alice,
            "Plan",
            &["bob@example.com", "carol@example.com"],
            &[],
        )
        .await;
    let path = format!("/api/activity?flare={flare}");
    api.faults.index.store(true, Ordering::SeqCst);
    let (status, body) = api
        .json(
            "POST",
            "/api/shares",
            Some(&bob),
            share_body("Counter-plan", &["carol@example.com"], &[&flare]),
        )
        .await;
    assert_eq!((status, code(&body)), (409, "flare::shares::partly_shared"));
    let response = body["share"].as_str().unwrap().to_owned();
    api.faults.index.store(false, Ordering::SeqCst);
    let (_, missing) = api.read("GET", &path, Some(&carol)).await;
    assert_eq!(ids(&missing), Vec::<String>::new());
    let (status, _) = api
        .json(
            "POST",
            &format!("/api/grants/{response}"),
            Some(&bob),
            json!({"with": ["carol@example.com"], "re": [flare]}),
        )
        .await;
    assert_eq!(status, 200);
    for reader in [&alice, &bob, &carol] {
        let (_, page) = api.read("GET", &path, Some(reader)).await;
        assert_eq!(ids(&page), vec![response.clone()]);
    }
}

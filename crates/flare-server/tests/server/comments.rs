//! Comments, replies and named cursors on the hosted site: the thread, the
//! author rules, activity that merges comments with responses, and the
//! browser's cookie access with its header checks. Each refusal test also
//! shows the admitted case.

use crate::api::Api;
use serde_json::{Value, json};

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

fn kinds(page: &Value) -> Vec<&str> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["kind"].as_str().unwrap())
        .collect()
}

async fn post(
    api: &Api,
    token: &str,
    flare: &str,
    parent: Option<&str>,
    key: &str,
) -> (u16, Value) {
    let mut body = json!({"flare": flare, "body": format!("text {key}"), "key": key});
    if let Some(parent) = parent {
        body["parent"] = json!(parent);
    }
    api.json("POST", "/api/comments", Some(token), body).await
}

/// Alice's flare, shared with bob, with alice's token and bob's.
async fn pair(api: &Api) -> (String, String, String) {
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    let flare = api.share(&alice, "Plan", &["bob@example.com"], &[]).await;
    (alice, bob, flare)
}

#[tokio::test]
async fn a_person_who_can_open_a_flare_comments_and_replies_and_the_thread_nests_the_reply() {
    let api = Api::start().await;
    let (alice, bob, flare) = pair(&api).await;
    let (status, comment) = post(&api, &alice, &flare, None, "one").await;
    assert_eq!(status, 201, "{comment}");
    assert_eq!(comment["kind"], "comment");
    assert_eq!(comment["author"], "alice@example.com");
    let id = comment["id"].as_str().unwrap();
    let (status, reply) = post(&api, &bob, &flare, Some(id), "two").await;
    assert_eq!(status, 201, "{reply}");
    assert_eq!(reply["kind"], "reply");
    assert_eq!(reply["parent"], id);
    let (status, thread) = api
        .read("GET", &format!("/api/comments?flare={flare}"), Some(&bob))
        .await;
    assert_eq!(status, 200, "{thread}");
    assert_eq!(thread["flare"], flare.as_str());
    let first = &thread["comments"][0];
    assert_eq!(first["id"], id);
    assert_eq!(first["body"], "text one");
    assert_eq!(first["deleted"], false);
    assert_eq!(first["replies"][0]["id"], reply["id"]);
    assert_eq!(first["replies"][0]["body"], "text two");
}

#[tokio::test]
async fn a_reply_to_a_reply_is_refused_and_a_reply_to_the_comment_is_admitted() {
    let api = Api::start().await;
    let (alice, _bob, flare) = pair(&api).await;
    let (_, comment) = post(&api, &alice, &flare, None, "one").await;
    let (_, reply) = post(&api, &alice, &flare, comment["id"].as_str(), "two").await;
    let (status, refused) = post(&api, &alice, &flare, reply["id"].as_str(), "three").await;
    assert_eq!(
        (status, code(&refused)),
        (400, "flare::comments::reply_depth")
    );
    let (status, _) = post(&api, &alice, &flare, comment["id"].as_str(), "four").await;
    assert_eq!(status, 201);
}

#[tokio::test]
async fn the_same_key_answers_the_first_comment_with_200_and_writes_nothing_more() {
    let api = Api::start().await;
    let (alice, _bob, flare) = pair(&api).await;
    let (first_status, first) = post(&api, &alice, &flare, None, "same").await;
    let (second_status, second) = post(&api, &alice, &flare, None, "same").await;
    assert_eq!((first_status, second_status), (201, 200));
    assert_eq!(first["id"], second["id"]);
    let (_, thread) = api
        .read("GET", &format!("/api/comments?flare={flare}"), Some(&alice))
        .await;
    assert_eq!(thread["comments"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_body_outside_1_to_8000_characters_or_a_bad_key_is_refused() {
    let api = Api::start().await;
    let (alice, _bob, flare) = pair(&api).await;
    for body in [String::new(), "x".repeat(8_001)] {
        let (status, refused) = api
            .json(
                "POST",
                "/api/comments",
                Some(&alice),
                json!({"flare": flare, "body": body, "key": "k"}),
            )
            .await;
        assert_eq!((status, code(&refused)), (400, "flare::comments::invalid"));
    }
    let (status, refused) = api
        .json(
            "POST",
            "/api/comments",
            Some(&alice),
            json!({"flare": flare, "body": "ok", "key": "has space"}),
        )
        .await;
    assert_eq!((status, code(&refused)), (400, "flare::comments::invalid"));
    let (status, _) = api
        .json(
            "POST",
            "/api/comments",
            Some(&alice),
            json!({"flare": flare, "body": "x".repeat(8_000), "key": "k"}),
        )
        .await;
    assert_eq!(status, 201);
}

#[tokio::test]
async fn only_the_author_edits_or_deletes_and_a_delete_erases_the_text_and_keeps_the_replies() {
    let api = Api::start().await;
    let (alice, bob, flare) = pair(&api).await;
    let (_, comment) = post(&api, &alice, &flare, None, "one").await;
    let id = comment["id"].as_str().unwrap();
    let (_, reply) = post(&api, &bob, &flare, Some(id), "two").await;
    let path = format!("/api/comments/{id}");
    let (status, refused) = api
        .json("PATCH", &path, Some(&bob), json!({"body": "mine now"}))
        .await;
    assert_eq!(
        (status, code(&refused)),
        (403, "flare::comments::not_author")
    );
    let (status, refused) = api.read("DELETE", &path, Some(&bob)).await;
    assert_eq!(
        (status, code(&refused)),
        (403, "flare::comments::not_author")
    );
    let (status, edited) = api
        .json("PATCH", &path, Some(&alice), json!({"body": "better"}))
        .await;
    assert_eq!(status, 200, "{edited}");
    assert_eq!(edited["body"], "better");
    assert!(edited["edited"].is_string());
    let (status, deleted) = api.read("DELETE", &path, Some(&alice)).await;
    assert_eq!(status, 200, "{deleted}");
    assert_eq!(deleted["deleted"], true);
    assert!(deleted.get("body").is_none());
    let (_, thread) = api
        .read("GET", &format!("/api/comments?flare={flare}"), Some(&bob))
        .await;
    let first = &thread["comments"][0];
    assert_eq!(first["deleted"], true);
    assert!(first.get("body").is_none());
    assert_eq!(first["replies"][0]["id"], reply["id"]);
    assert_eq!(first["replies"][0]["body"], "text two");
    let (status, refused) = api
        .json("PATCH", &path, Some(&alice), json!({"body": "again"}))
        .await;
    assert_eq!(
        (status, code(&refused)),
        (404, "flare::comments::not_found")
    );
}

#[tokio::test]
async fn a_flare_the_caller_cannot_open_and_a_missing_one_give_the_same_refusal() {
    let api = Api::start().await;
    let (alice, _bob, flare) = pair(&api).await;
    let carol = api.token_for("carol").await;
    let (_, comment) = post(&api, &alice, &flare, None, "one").await;
    let missing = "0123456789abcdef0123456789abcdef";
    let mut bodies = Vec::new();
    for target in [flare.as_str(), missing] {
        let (status, read) = api
            .read(
                "GET",
                &format!("/api/comments?flare={target}"),
                Some(&carol),
            )
            .await;
        let (wrote, write) = post(&api, &carol, target, None, "x").await;
        assert_eq!((status, wrote), (404, 404));
        bodies.push((read, write));
    }
    assert_eq!(bodies[0], bodies[1]);
    let id = comment["id"].as_str().unwrap();
    let (status, refused) = api
        .json(
            "PATCH",
            &format!("/api/comments/{id}"),
            Some(&carol),
            json!({"body": "x"}),
        )
        .await;
    assert_eq!(
        (status, code(&refused)),
        (404, "flare::comments::not_found")
    );
}

#[tokio::test]
async fn activity_returns_comments_and_responses_in_order_and_names_edits_and_deletes() {
    let api = Api::start().await;
    let (alice, bob, flare) = pair(&api).await;
    let (_, comment) = post(&api, &alice, &flare, None, "one").await;
    let id = comment["id"].as_str().unwrap();
    api.share(&bob, "Counter", &["alice@example.com"], &[&flare])
        .await;
    post(&api, &bob, &flare, Some(id), "two").await;
    api.json(
        "PATCH",
        &format!("/api/comments/{id}"),
        Some(&alice),
        json!({"body": "edited"}),
    )
    .await;
    let (_, gone) = post(&api, &bob, &flare, Some(id), "three").await;
    let gone = gone["id"].as_str().unwrap();
    api.read("DELETE", &format!("/api/comments/{gone}"), Some(&bob))
        .await;
    let (status, page) = api
        .read("GET", &format!("/api/activity?flare={flare}"), Some(&alice))
        .await;
    assert_eq!(status, 200, "{page}");
    // The reply that was deleted holds no text, so only its deletion shows.
    assert_eq!(
        kinds(&page),
        ["comment", "response", "reply", "edited", "deleted"]
    );
    let items = page["items"].as_array().unwrap();
    assert_eq!(items[0]["body"], "edited");
    assert_eq!(items[2]["parent"], id);
    assert_eq!(items[2]["body"], "text two");
    assert_eq!(items[3]["body"], "edited");
    assert_eq!(items[4]["comment"], gone);
    assert_eq!(items[4]["parent"], id);
    assert!(items[4].get("body").is_none());
}

#[tokio::test]
async fn since_last_reads_from_the_saved_cursor_and_a_missing_cursor_answers_not_found() {
    let api = Api::start().await;
    let (alice, bob, flare) = pair(&api).await;
    post(&api, &alice, &flare, None, "one").await;
    let (status, missing) = api.read("GET", "/api/cursors/inbox", Some(&bob)).await;
    assert_eq!((status, code(&missing)), (404, "flare::cursors::not_found"));
    let activity = format!("/api/activity?flare={flare}&since_last=inbox");
    let (_, all) = api.read("GET", &activity, Some(&bob)).await;
    assert_eq!(kinds(&all), ["comment"]);
    let cursor = all["cursor"].as_str().unwrap().to_owned();
    let (status, saved) = api
        .json(
            "PUT",
            "/api/cursors/inbox",
            Some(&bob),
            json!({"cursor": cursor}),
        )
        .await;
    assert_eq!(
        (status, saved["cursor"].as_str()),
        (200, Some(cursor.as_str()))
    );
    let (_, none) = api.read("GET", &activity, Some(&bob)).await;
    assert_eq!(kinds(&none), Vec::<&str>::new());
    post(&api, &alice, &flare, None, "two").await;
    let (_, newer) = api.read("GET", &activity, Some(&bob)).await;
    assert_eq!(kinds(&newer), ["comment"]);
    let (_, kept) = api.read("GET", "/api/cursors/inbox", Some(&bob)).await;
    assert_eq!(kept["cursor"], cursor.as_str());
    let (status, other) = api.read("GET", "/api/cursors/inbox", Some(&alice)).await;
    assert_eq!((status, code(&other)), (404, "flare::cursors::not_found"));
    let (status, bad) = api
        .json(
            "PUT",
            "/api/cursors/inbox",
            Some(&bob),
            json!({"cursor": "fc9.nope"}),
        )
        .await;
    assert_eq!((status, code(&bad)), (400, "flare::activity::bad_cursor"));
}

#[tokio::test]
async fn an_opened_flare_counts_its_live_comments() {
    let api = Api::start().await;
    let (alice, _bob, flare) = pair(&api).await;
    let (_, one) = post(&api, &alice, &flare, None, "one").await;
    post(&api, &alice, &flare, None, "two").await;
    api.read(
        "DELETE",
        &format!("/api/comments/{}", one["id"].as_str().unwrap()),
        Some(&alice),
    )
    .await;
    let (_, opened) = api
        .read("GET", &format!("/api/shares/{flare}"), Some(&alice))
        .await;
    assert_eq!(opened["share"]["comments"], 1);
}

#[tokio::test]
async fn a_signed_in_browser_reads_and_writes_comments_with_its_cookie_and_same_origin_headers() {
    let api = Api::start().await;
    let (alice, _bob, flare) = pair(&api).await;
    let cookie = api.session("bob").await;
    let body = json!({"flare": flare, "body": "from the page", "key": "page1"}).to_string();
    let wrote = api
        .browser("POST", "/api/comments", &cookie)
        .body("application/json", body.into_bytes())
        .send()
        .await;
    assert_eq!(wrote.status, 201, "{}", wrote.body);
    let read = api
        .browser("GET", &format!("/api/comments?flare={flare}"), &cookie)
        .send()
        .await;
    assert_eq!(read.status, 200, "{}", read.body);
    assert!(read.body.contains("from the page") && read.body.contains("bob@example.com"));
    let activity = api
        .browser("GET", &format!("/api/activity?flare={flare}"), &cookie)
        .send()
        .await;
    assert_eq!(activity.status, 200, "{}", activity.body);
    let (_, thread) = api
        .read("GET", &format!("/api/comments?flare={flare}"), Some(&alice))
        .await;
    assert_eq!(thread["comments"][0]["author"], "bob@example.com");
}

#[tokio::test]
async fn a_cookie_request_without_the_headers_a_page_sends_is_refused_before_routing() {
    let api = Api::start().await;
    let (_alice, _bob, flare) = pair(&api).await;
    let cookie = api.session("bob").await;
    let body = json!({"flare": flare, "body": "x", "key": "k"}).to_string();
    let write =
        |call: crate::harness::Call| call.body("application/json", body.clone().into_bytes());
    let refused = |reply: &crate::harness::Reply| (reply.status, reply.code());
    let origin = Some("request::origin".to_owned());
    let hosted = &api.hosted;
    // No Sec-Fetch-Site.
    let reply = write(
        hosted
            .request("POST", "/api/comments")
            .cookie(&cookie)
            .header("Origin", &hosted.origin),
    )
    .send()
    .await;
    assert_eq!(refused(&reply), (403, origin.clone()));
    // Another site's Origin.
    let reply = write(
        hosted
            .request("POST", "/api/comments")
            .cookie(&cookie)
            .header("Origin", "https://evil.example")
            .header("Sec-Fetch-Site", "same-origin"),
    )
    .send()
    .await;
    assert_eq!(refused(&reply), (403, origin.clone()));
    // A form content type, which a cross-site form can send.
    let reply = hosted
        .request("POST", "/api/comments")
        .cookie(&cookie)
        .same_origin()
        .body(
            "application/x-www-form-urlencoded",
            body.clone().into_bytes(),
        )
        .send()
        .await;
    assert_eq!(refused(&reply), (403, origin.clone()));
    // A read from another site.
    let reply = hosted
        .request("GET", &format!("/api/comments?flare={flare}"))
        .cookie(&cookie)
        .header("Sec-Fetch-Site", "cross-site")
        .send()
        .await;
    assert_eq!(refused(&reply), (403, origin.clone()));
    let reply = hosted
        .request("GET", &format!("/api/comments?flare={flare}"))
        .cookie(&cookie)
        .send()
        .await;
    assert_eq!(refused(&reply), (403, origin));
    // The admitted twin.
    let reply = api
        .browser("POST", "/api/comments", &cookie)
        .body("application/json", body.into_bytes())
        .send()
        .await;
    assert_eq!(reply.status, 201, "{}", reply.body);
}

#[tokio::test]
async fn a_bearer_request_reads_no_cookie_and_a_cookie_opens_no_other_route() {
    let api = Api::start().await;
    let (alice, bob, flare) = pair(&api).await;
    let alice_cookie = api.session("alice").await;
    // Bob's token with Alice's cookie acts as Bob.
    let reply = api
        .call("POST", "/api/comments", Some(&bob))
        .cookie(&alice_cookie)
        .same_origin()
        .body(
            "application/json",
            json!({"flare": flare, "body": "x", "key": "k"})
                .to_string()
                .into_bytes(),
        )
        .send()
        .await;
    assert_eq!(reply.status, 201, "{}", reply.body);
    assert!(reply.body.contains("bob@example.com"));
    // A malformed bearer with a good cookie is refused, not rescued.
    let reply = api
        .browser(
            "GET",
            &format!("/api/comments?flare={flare}"),
            &alice_cookie,
        )
        .header("Authorization", "Bearer not-a-token!")
        .send()
        .await;
    assert_eq!(reply.status, 401, "{}", reply.body);
    // The cookie opens neither the share routes nor the cursor routes.
    for path in ["/api/shares?scope=mine", "/api/cursors/inbox"] {
        let reply = api.browser("GET", path, &alice_cookie).send().await;
        assert_eq!(
            (reply.status, reply.code()),
            (401, Some("flare::tokens::sign_in_needed".to_owned())),
            "{path}"
        );
    }
    let (status, _) = api
        .read("GET", "/api/shares?scope=mine", Some(&alice))
        .await;
    assert_eq!(status, 200);
    // A request with neither is refused.
    let (status, body) = api
        .read("GET", &format!("/api/comments?flare={flare}"), None)
        .await;
    assert_eq!(
        (status, code(&body)),
        (401, "flare::tokens::sign_in_needed")
    );
}

#[tokio::test]
async fn hosted_replies_allow_connections_to_their_own_origin_and_local_ones_do_not() {
    let api = Api::start().await;
    let reply = api.hosted.get("/up").send().await;
    let policy = reply.header("content-security-policy").unwrap();
    assert!(policy.contains("connect-src 'self'"), "{policy}");
    assert!(policy.starts_with("default-src 'none'"));
}

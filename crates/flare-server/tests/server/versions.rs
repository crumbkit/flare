//! `POST /api/shares/<id>/versions`: the owner publishes a new version of a
//! share. Each refusal test shows the owner's admitted post beside it.

use crate::api::{Api, parse};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

fn version_body(content: &str) -> Value {
    json!({"format": "markdown", "content": content})
}

fn path(id: &str) -> String {
    format!("/api/shares/{id}/versions")
}

#[tokio::test]
async fn the_owner_publishes_a_second_version_and_every_opener_sees_it_at_once() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    let id = api.share(&alice, "Plan", &["bob@example.com"], &[]).await;
    let (status, published) = api
        .json(
            "POST",
            &path(&id),
            Some(&alice),
            version_body("# Plan v2\n"),
        )
        .await;
    assert_eq!(status, 201, "{published}");
    assert_eq!(published["id"], json!(id));
    assert_eq!(published["version"], json!(2));
    assert_eq!(published["role"], json!("owner"));
    for token in [&alice, &bob] {
        let (status, opened) = api
            .read("GET", &format!("/api/shares/{id}"), Some(token))
            .await;
        assert_eq!(status, 200);
        assert_eq!(opened["content"], json!("# Plan v2\n"));
        assert_eq!(opened["share"]["version"], json!(2));
    }
    let (status, third) = api
        .json(
            "POST",
            &path(&id),
            Some(&alice),
            version_body("# Plan v3\n"),
        )
        .await;
    assert_eq!((status, &third["version"]), (201, &json!(3)));
}

#[tokio::test]
async fn a_reader_is_forbidden_and_a_stranger_and_an_unknown_id_get_the_same_not_found() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    let carol = api.token_for("carol").await;
    let id = api.share(&alice, "Plan", &["bob@example.com"], &[]).await;
    let (status, refused) = api
        .json("POST", &path(&id), Some(&bob), version_body("# Mine\n"))
        .await;
    assert_eq!((status, code(&refused)), (403, "flare::shares::forbidden"));
    let stranger = api
        .call("POST", &path(&id), Some(&carol))
        .body(
            "application/json",
            version_body("# Mine\n").to_string().into_bytes(),
        )
        .send()
        .await;
    let unknown = api
        .call("POST", &path(&"0".repeat(32)), Some(&carol))
        .body(
            "application/json",
            version_body("# Mine\n").to_string().into_bytes(),
        )
        .send()
        .await;
    assert_eq!(stranger.status, 404);
    assert_eq!(parse(&stranger)["code"], json!("flare::shares::not_found"));
    assert_eq!(stranger.status, unknown.status);
    assert_eq!(
        parse(&stranger)["detail"],
        parse(&unknown)["detail"],
        "the answers differ"
    );
    let (_, opened) = api
        .read("GET", &format!("/api/shares/{id}"), Some(&alice))
        .await;
    assert_eq!(opened["share"]["version"], json!(1));
    let (status, _) = api
        .json("POST", &path(&id), Some(&alice), version_body("# Owner\n"))
        .await;
    assert_eq!(status, 201);
}

#[tokio::test]
async fn a_share_at_the_version_limit_names_versions_and_does_not_offer_an_operator_raise() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let id = api.share(&alice, "Plan", &[], &[]).await;
    for number in 2..=100 {
        let body = version_body(&format!("v{number}\n"));
        api.json("POST", &path(&id), Some(&alice), body).await;
    }
    let (_, refused) = api
        .json("POST", &path(&id), Some(&alice), version_body("v101\n"))
        .await;
    assert!(
        refused["title"].as_str().unwrap().contains("version"),
        "{refused}"
    );
    assert!(
        !refused["recovery"].to_string().contains("operator"),
        "{refused}"
    );
}

#[tokio::test]
async fn a_post_with_no_token_or_a_refused_token_writes_nothing() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let id = api.share(&alice, "Plan", &[], &[]).await;
    for token in [None, Some("flare_nonsense")] {
        let (status, body) = api
            .json("POST", &path(&id), token, version_body("# Nope\n"))
            .await;
        assert_eq!(
            (status, code(&body)),
            (401, "flare::tokens::sign_in_needed")
        );
    }
    let (_, opened) = api
        .read("GET", &format!("/api/shares/{id}"), Some(&alice))
        .await;
    assert_eq!(opened["share"]["version"], json!(1));
}

#[tokio::test]
async fn html_an_empty_body_and_an_oversized_body_are_refused_and_store_nothing() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let id = api.share(&alice, "Plan", &[], &[]).await;
    let cases = [
        (
            json!({"format": "html", "content": "<p>x</p>"}),
            415,
            "flare::shares::kind_not_served",
        ),
        (version_body(""), 400, "flare::shares::invalid"),
        (
            version_body(&"x".repeat(262_145)),
            413,
            "flare::shares::too_large",
        ),
    ];
    for (body, expected_status, expected) in cases {
        let (status, refused) = api.json("POST", &path(&id), Some(&alice), body).await;
        assert_eq!((status, code(&refused)), (expected_status, expected));
    }
    let (_, opened) = api
        .read("GET", &format!("/api/shares/{id}"), Some(&alice))
        .await;
    assert_eq!(opened["share"]["version"], json!(1));
    let (status, _) = api
        .json(
            "POST",
            &path(&id),
            Some(&alice),
            version_body(&"x".repeat(262_144)),
        )
        .await;
    assert_eq!(status, 201);
}

#[tokio::test]
async fn a_body_with_a_title_or_that_is_not_json_is_a_bad_body() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let id = api.share(&alice, "Plan", &[], &[]).await;
    let (status, refused) = api
        .json(
            "POST",
            &path(&id),
            Some(&alice),
            json!({"format": "markdown", "content": "# x\n", "title": "New"}),
        )
        .await;
    assert_eq!((status, code(&refused)), (400, "flare::requests::bad_body"));
    let reply = api
        .call("POST", &path(&id), Some(&alice))
        .body("application/json", b"{not json".to_vec())
        .send()
        .await;
    assert_eq!(reply.status, 400);
}

#[tokio::test]
async fn a_share_at_one_hundred_versions_refuses_another_and_stays_unchanged() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let id = api.share(&alice, "Plan", &[], &[]).await;
    for number in 2..=100 {
        let (status, published) = api
            .json(
                "POST",
                &path(&id),
                Some(&alice),
                version_body(&format!("v{number}\n")),
            )
            .await;
        assert_eq!((status, &published["version"]), (201, &json!(number)));
    }
    let (status, refused) = api
        .json("POST", &path(&id), Some(&alice), version_body("v101\n"))
        .await;
    assert_eq!((status, code(&refused)), (409, "flare::versions::too_many"));
    let (_, opened) = api
        .read("GET", &format!("/api/shares/{id}"), Some(&alice))
        .await;
    assert_eq!(opened["content"], json!("v100\n"));
}

#[tokio::test]
async fn a_failing_share_store_gives_a_5xx_problem_and_the_post_works_again_once_it_recovers() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let id = api.share(&alice, "Plan", &[], &[]).await;
    api.faults.shares.store(true, Ordering::SeqCst);
    let (status, body) = api
        .json("POST", &path(&id), Some(&alice), version_body("# v2\n"))
        .await;
    assert_eq!((status, code(&body)), (503, "flare::shares::unavailable"));
    api.faults.shares.store(false, Ordering::SeqCst);
    let (status, published) = api
        .json("POST", &path(&id), Some(&alice), version_body("# v2\n"))
        .await;
    assert_eq!((status, &published["version"]), (201, &json!(2)));
}

#[tokio::test]
async fn opening_with_a_version_returns_that_versions_content_and_without_one_the_latest() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    let id = api.share(&alice, "Plan", &["bob@example.com"], &[]).await;
    let (status, _) = api
        .json(
            "POST",
            &path(&id),
            Some(&alice),
            version_body("# Plan v2\n"),
        )
        .await;
    assert_eq!(status, 201);
    for token in [&alice, &bob] {
        let (status, first) = api
            .read("GET", &format!("/api/shares/{id}?version=1"), Some(token))
            .await;
        assert_eq!(status, 200, "{first}");
        assert_eq!(first["content"], json!("# Plan\n"));
        assert_eq!(first["version"], json!(1));
        assert_eq!(first["share"]["version"], json!(2));
        let (status, latest) = api
            .read("GET", &format!("/api/shares/{id}"), Some(token))
            .await;
        assert_eq!(status, 200, "{latest}");
        assert_eq!(latest["content"], json!("# Plan v2\n"));
        assert_eq!(latest["version"], json!(2));
    }
}

#[tokio::test]
async fn a_version_above_the_latest_zero_or_not_a_number_is_a_problem_with_no_content() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let id = api.share(&alice, "Plan", &[], &[]).await;
    let cases = [
        ("2", 404, "flare::shares::not_found"),
        ("0", 400, "flare::requests::bad_query"),
        ("abc", 400, "flare::requests::bad_query"),
        ("-1", 400, "flare::requests::bad_query"),
    ];
    for (version, expected_status, expected) in cases {
        let (status, refused) = api
            .read(
                "GET",
                &format!("/api/shares/{id}?version={version}"),
                Some(&alice),
            )
            .await;
        assert_eq!(
            (status, code(&refused)),
            (expected_status, expected),
            "{version}"
        );
        assert!(refused.get("content").is_none(), "{refused}");
    }
    let (status, opened) = api
        .read("GET", &format!("/api/shares/{id}?version=1"), Some(&alice))
        .await;
    assert_eq!((status, &opened["version"]), (200, &json!(1)));
}

#[tokio::test]
async fn a_stranger_and_an_unknown_id_get_the_same_answer_for_a_version() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let carol = api.token_for("carol").await;
    let id = api.share(&alice, "Plan", &[], &[]).await;
    for version in ["1", "2"] {
        let stranger = api
            .call(
                "GET",
                &format!("/api/shares/{id}?version={version}"),
                Some(&carol),
            )
            .send()
            .await;
        let unknown = api
            .call(
                "GET",
                &format!("/api/shares/{}?version={version}", "0".repeat(32)),
                Some(&carol),
            )
            .send()
            .await;
        assert_eq!(stranger.status, 404);
        assert_eq!(stranger.status, unknown.status);
        assert_eq!(parse(&stranger), parse(&unknown), "the answers differ");
    }
    let (status, _) = api
        .read("GET", &format!("/api/shares/{id}?version=1"), Some(&alice))
        .await;
    assert_eq!(status, 200);
}

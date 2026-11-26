//! `GET /api/search` and `GET /api/search/fields` on the hosted site: a
//! valid query answers the shares the caller can open, an invalid one
//! answers 422 with every error and runs nothing, and the route needs a
//! token. Each refusal test also shows the admitted case.

use crate::api::Api;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::time::Duration;

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

fn titles(answer: &Value) -> Vec<&str> {
    answer["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|share| share["title"].as_str().unwrap())
        .collect()
}

/// A server where alice shared two flares with bob and bob one with no
/// one, and bob's token.
async fn shared() -> (Api, String) {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    let flares: [(&str, &str, &[&str]); 4] = [
        (&alice, "Budget review", &["bob@example.com"]),
        (&alice, "Team notes", &["bob@example.com"]),
        (&alice, "Secret budget", &[]),
        (&bob, "Budget draft", &[]),
    ];
    for (token, title, with) in flares {
        // Each share is created a second after the one before, so the
        // newest-first order does not fall back to the random ids.
        api.hosted.time.advance(Duration::from_secs(1));
        api.share(token, title, with, &[]).await;
    }
    (api, bob)
}

#[tokio::test]
async fn a_search_answers_the_canonical_query_its_parts_and_the_shares_the_caller_can_open() {
    let (api, bob) = shared().await;
    let (status, answer) = api
        .read(
            "GET",
            "/api/search?q=BUDGET%20%20From:me,alice@example.com",
            Some(&bob),
        )
        .await;
    assert_eq!(status, 200, "{answer}");
    assert_eq!(answer["query"], "BUDGET from:me,alice@example.com");
    assert_eq!(answer["parsed"][0]["kind"], "word");
    assert_eq!(answer["parsed"][0]["span"], json!({"start": 0, "end": 6}));
    assert_eq!(answer["parsed"][1]["field"], "from");
    assert_eq!(
        answer["parsed"][1]["values"][1]["text"],
        "alice@example.com"
    );
    assert_eq!(titles(&answer), ["Budget draft", "Budget review"]);
    assert_eq!(answer["next"], Value::Null);
    assert_eq!(answer["partial"], false);
    let (_, list) = api
        .read("GET", "/api/shares?scope=with-me", Some(&bob))
        .await;
    let review = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|share| share["title"] == "Budget review")
        .unwrap();
    assert_eq!(&answer["results"][1], review);
}

#[tokio::test]
async fn an_invalid_query_answers_422_with_every_error_and_its_valid_twin_answers_200() {
    let (api, bob) = shared().await;
    let reply = api
        .call("GET", "/api/search?q=form:erin%20%22open", Some(&bob))
        .send()
        .await;
    assert_eq!(reply.status, 422, "{}", reply.body);
    assert_eq!(
        reply.header("content-type"),
        Some("application/problem+json")
    );
    let problem: Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(code(&problem), "flare::search::invalid");
    let errors = problem["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0]["code"], "flare::search::unknown_field");
    assert_eq!(errors[0]["span"], json!({"start": 0, "end": 4}));
    assert_eq!(
        errors[0]["message"],
        "There is no filter called form; did you mean from?"
    );
    assert_eq!(errors[1]["code"], "flare::search::unclosed_quote");
    assert_eq!(errors[1]["span"], json!({"start": 10, "end": 15}));
    let (status, _) = api
        .read(
            "GET",
            "/api/search?q=from:alice@example.com%20%22open%22",
            Some(&bob),
        )
        .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn an_unknown_type_is_invalid_and_an_unknown_person_has_its_own_code() {
    let (api, bob) = shared().await;
    let (status, problem) = api.read("GET", "/api/search?q=type:pdf", Some(&bob)).await;
    assert_eq!(status, 422);
    assert_eq!(code(&problem), "flare::search::invalid");
    assert_eq!(problem["errors"][0]["code"], "flare::search::unknown_type");
    let (status, problem) = api
        .read("GET", "/api/search?q=from:nobody", Some(&bob))
        .await;
    assert_eq!(status, 422);
    assert_eq!(code(&problem), "flare::search::unknown_person");
    assert_eq!(problem["errors"][0]["span"], json!({"start": 5, "end": 11}));
    let (status, answer) = api
        .read("GET", "/api/search?q=type:markdown%20from:me", Some(&bob))
        .await;
    assert_eq!(status, 200);
    assert_eq!(titles(&answer), ["Budget draft"]);
}

#[tokio::test]
async fn a_search_without_a_token_answers_401_and_with_one_answers_200() {
    let (api, bob) = shared().await;
    let (status, problem) = api.read("GET", "/api/search?q=budget", None).await;
    assert_eq!(status, 401);
    assert_eq!(code(&problem), "flare::tokens::sign_in_needed");
    let (status, problem) = api.read("GET", "/api/search/fields", None).await;
    assert_eq!(status, 401, "{problem}");
    let (status, _) = api.read("GET", "/api/search?q=budget", Some(&bob)).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn a_search_needs_one_q_and_a_limit_from_1_to_500() {
    let (api, bob) = shared().await;
    for query in ["", "?q=a&q=b", "?q=budget&limit=0", "?q=budget&limit=501"] {
        let (status, problem) = api
            .read("GET", &format!("/api/search{query}"), Some(&bob))
            .await;
        assert_eq!(status, 400, "{query}");
        assert_eq!(code(&problem), "flare::requests::bad_query", "{query}");
    }
    let (status, answer) = api.read("GET", "/api/search?q=&limit=1", Some(&bob)).await;
    assert_eq!(status, 200);
    assert_eq!(titles(&answer), ["Budget draft"]);
}

#[tokio::test]
async fn a_store_or_directory_that_does_not_answer_answers_503_and_a_working_one_answers() {
    let (api, bob) = shared().await;
    api.faults.recipients.store(true, Ordering::SeqCst);
    let (status, problem) = api
        .read("GET", "/api/search?q=from:alice@example.com", Some(&bob))
        .await;
    assert_eq!(status, 503);
    assert_eq!(code(&problem), "flare::recipients::unavailable");
    api.faults.recipients.store(false, Ordering::SeqCst);
    api.faults.shares.store(true, Ordering::SeqCst);
    let (status, problem) = api.read("GET", "/api/search?q=budget", Some(&bob)).await;
    assert_eq!(status, 503);
    assert_eq!(code(&problem), "flare::shares::unavailable");
    api.faults.shares.store(false, Ordering::SeqCst);
    let (status, _) = api.read("GET", "/api/search?q=budget", Some(&bob)).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn the_fields_route_lists_type_with_its_values_and_from() {
    let (api, bob) = shared().await;
    let (status, fields) = api.read("GET", "/api/search/fields", Some(&bob)).await;
    assert_eq!(status, 200, "{fields}");
    assert_eq!(fields["fields"][0]["name"], "type");
    assert_eq!(fields["fields"][0]["kind"], "keyword");
    assert_eq!(fields["fields"][0]["values"], json!(["markdown", "html"]));
    assert_eq!(fields["fields"][1]["name"], "from");
    assert_eq!(fields["fields"][1]["kind"], "person");
}

//! Comments on highlighted text through the hosted API: the anchor a
//! comment carries, its refusals, and the delete that erases it. Each
//! refusal also shows the admitted case.

use crate::api::Api;
use serde_json::{Value, json};

const BOUNDARY: &str = "flare-test-anchor";

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

fn anchor(quote: &str) -> Value {
    json!({
        "quote": quote,
        "prefix": "above the limit, ",
        "suffix": " and passengers",
        "version": 1,
    })
}

async fn post(api: &Api, token: &str, body: Value) -> (u16, Value) {
    api.json("POST", "/api/comments", Some(token), body).await
}

fn comment(flare: &str, key: &str, anchor: Option<Value>) -> Value {
    let mut body = json!({"flare": flare, "body": format!("text {key}"), "key": key});
    if let Some(anchor) = anchor {
        body["anchor"] = anchor;
    }
    body
}

async fn thread(api: &Api, token: &str, flare: &str) -> Value {
    let (status, thread) = api
        .read("GET", &format!("/api/comments?flare={flare}"), Some(token))
        .await;
    assert_eq!(status, 200, "{thread}");
    thread
}

/// Alice's Markdown flare, shared with bob, with alice's token.
async fn flare(api: &Api) -> (String, String) {
    let alice = api.token_for("alice").await;
    let flare = api.share(&alice, "Plan", &["bob@example.com"], &[]).await;
    (alice, flare)
}

#[tokio::test]
async fn an_anchored_comment_answers_and_reads_back_with_its_anchor_and_a_repeat_keeps_it() {
    let api = Api::start().await;
    let (alice, flare) = flare(&api).await;
    let sent = anchor("the sailing is held");
    let (status, posted) = post(&api, &alice, comment(&flare, "one", Some(sent.clone()))).await;
    assert_eq!(status, 201, "{posted}");
    assert_eq!(posted["anchor"], sent);
    let (status, repeated) = post(&api, &alice, comment(&flare, "one", Some(sent.clone()))).await;
    assert_eq!((status, &repeated["anchor"]), (200, &sent));
    let (_, whole) = post(&api, &alice, comment(&flare, "two", None)).await;
    assert!(whole.get("anchor").is_none(), "{whole}");
    let read = thread(&api, &alice, &flare).await;
    assert_eq!(read["comments"][0]["anchor"], sent);
    assert!(read["comments"][1].get("anchor").is_none(), "{read}");
}

#[tokio::test]
async fn an_anchor_on_a_reply_is_refused_and_the_same_reply_without_one_is_admitted() {
    let api = Api::start().await;
    let (alice, flare) = flare(&api).await;
    let (_, first) = post(&api, &alice, comment(&flare, "one", None)).await;
    let mut reply = comment(&flare, "two", Some(anchor("the sailing is held")));
    reply["parent"] = first["id"].clone();
    let (status, refused) = post(&api, &alice, reply.clone()).await;
    assert_eq!(
        (status, code(&refused)),
        (400, "flare::comments::anchor_on_reply")
    );
    reply.as_object_mut().unwrap().remove("anchor");
    let (status, admitted) = post(&api, &alice, reply).await;
    assert_eq!(status, 201, "{admitted}");
    let read = thread(&api, &alice, &flare).await;
    assert_eq!(read["comments"][0]["replies"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn an_anchor_outside_its_limits_is_refused_and_writes_nothing_and_one_at_its_limits_is_admitted()
 {
    let api = Api::start().await;
    let (alice, flare) = flare(&api).await;
    let long_prefix = {
        let mut over = anchor("the sailing is held");
        over["prefix"] = json!("x".repeat(65));
        over
    };
    let other_version = {
        let mut over = anchor("the sailing is held");
        over["version"] = json!(2);
        over
    };
    for refused_anchor in [
        anchor(""),
        anchor("ab"),
        anchor(&"x".repeat(501)),
        long_prefix,
        other_version,
    ] {
        let (status, refused) =
            post(&api, &alice, comment(&flare, "one", Some(refused_anchor))).await;
        assert_eq!(
            (status, code(&refused)),
            (400, "flare::comments::anchor_invalid"),
            "{refused}"
        );
    }
    assert_eq!(thread(&api, &alice, &flare).await["comments"], json!([]));
    let mut longest = anchor(&"x".repeat(500));
    longest["prefix"] = json!("x".repeat(64));
    let (status, admitted) = post(&api, &alice, comment(&flare, "one", Some(longest))).await;
    assert_eq!(status, 201, "{admitted}");
}

#[tokio::test]
async fn an_anchor_on_an_uploaded_html_flare_is_refused_and_the_same_comment_without_one_is_admitted()
 {
    let api = Api::start_with_content_host().await;
    let alice = api.token_for("alice").await;
    let share = json!({"title": "Plan", "format": "html", "content": "", "with": [], "re": []});
    let mut body = format!(
        "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"share\"\r\nContent-Type: application/json\r\n\r\n{share}\r\n--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"html\"; filename=\"plan.html\"\r\nContent-Type: text/html\r\n\r\n<!doctype html><title>Plan</title><p>Words</p>"
    )
    .into_bytes();
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    let reply = api
        .call("POST", "/api/shares", Some(&alice))
        .body(&format!("multipart/form-data; boundary={BOUNDARY}"), body)
        .send()
        .await;
    assert_eq!(reply.status, 201, "{}", reply.body);
    let shared: Value = serde_json::from_str(&reply.body).unwrap();
    let flare = shared["share"]["id"].as_str().unwrap().to_owned();
    let (status, refused) = post(
        &api,
        &alice,
        comment(&flare, "one", Some(anchor("Words and more"))),
    )
    .await;
    assert_eq!(
        (status, code(&refused)),
        (400, "flare::comments::anchor_kind")
    );
    let (status, admitted) = post(&api, &alice, comment(&flare, "one", None)).await;
    assert_eq!(status, 201, "{admitted}");
}

#[tokio::test]
async fn a_delete_erases_the_anchor_with_the_text_and_keeps_the_reply() {
    let api = Api::start().await;
    let (alice, flare) = flare(&api).await;
    let (_, first) = post(
        &api,
        &alice,
        comment(&flare, "one", Some(anchor("the sailing is held"))),
    )
    .await;
    let id = first["id"].as_str().unwrap();
    let mut reply = comment(&flare, "two", None);
    reply["parent"] = json!(id);
    post(&api, &alice, reply).await;
    let (status, deleted) = api
        .read("DELETE", &format!("/api/comments/{id}"), Some(&alice))
        .await;
    assert_eq!(status, 200, "{deleted}");
    assert!(deleted.get("anchor").is_none(), "{deleted}");
    let read = thread(&api, &alice, &flare).await;
    let held = &read["comments"][0];
    assert!(held.get("anchor").is_none(), "{read}");
    assert!(held.get("body").is_none(), "{read}");
    assert_eq!(held["replies"].as_array().unwrap().len(), 1);
}

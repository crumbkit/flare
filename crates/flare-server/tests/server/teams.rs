//! The team routes and group grants on the hosted site: creating and
//! managing a team, sharing with a team or with everyone, listing and
//! revoking grants, and what each refusal answers.

use crate::api::{Api, share_body};
use serde_json::{Value, json};

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

/// A server under a domain admit rule, where alice owns the team design
/// with bob as a member. Answers the server and the tokens of alice, bob
/// and carol.
async fn with_design() -> (Api, String, String, String) {
    let api = Api::start_with_domain().await;
    let alice = api.token_for("alice").await;
    let bob = api.token_for("bob").await;
    let carol = api.token_for("carol").await;
    let (status, created) = api
        .json(
            "POST",
            "/api/teams",
            Some(&alice),
            json!({"name": "design"}),
        )
        .await;
    assert_eq!(status, 201, "{created}");
    let (status, added) = api
        .json(
            "POST",
            "/api/teams/design/members",
            Some(&alice),
            json!({"with": ["bob@example.com"]}),
        )
        .await;
    assert_eq!(status, 200, "{added}");
    (api, alice, bob, carol)
}

#[tokio::test]
async fn a_creator_creates_a_team_and_its_summary_names_them_as_owner() {
    let api = Api::start_with_domain().await;
    let alice = api.token_for("alice").await;
    let (status, created) = api
        .json(
            "POST",
            "/api/teams",
            Some(&alice),
            json!({"name": "design"}),
        )
        .await;
    assert_eq!(status, 201, "{created}");
    assert_eq!(created["name"], "design");
    assert_eq!(created["owner"], "alice@example.com");
    assert_eq!(created["role"], "owner");
    assert_eq!(created["members"], 1);
    let (status, taken) = api
        .json(
            "POST",
            "/api/teams",
            Some(&alice),
            json!({"name": "design"}),
        )
        .await;
    assert_eq!(status, 201, "{taken}");
    assert_eq!(taken["members"], 1);
    let bob = api.token_for("bob").await;
    let (status, taken) = api
        .json("POST", "/api/teams", Some(&bob), json!({"name": "design"}))
        .await;
    assert_eq!(status, 409);
    assert_eq!(code(&taken), "flare::teams::name_taken");
    let (status, invalid) = api
        .json(
            "POST",
            "/api/teams",
            Some(&alice),
            json!({"name": "Design!"}),
        )
        .await;
    assert_eq!(status, 400);
    assert_eq!(code(&invalid), "flare::teams::invalid_name");
}

#[tokio::test]
async fn a_member_reads_the_team_and_a_stranger_gets_the_same_answer_as_for_a_missing_team() {
    let (api, _alice, bob, carol) = with_design().await;
    let (status, shown) = api.read("GET", "/api/teams/design", Some(&bob)).await;
    assert_eq!(status, 200, "{shown}");
    assert_eq!(shown["team"]["role"], "member");
    assert_eq!(
        shown["members"],
        json!(["alice@example.com", "bob@example.com"])
    );
    let (status, hidden) = api.read("GET", "/api/teams/design", Some(&carol)).await;
    let (missing_status, missing) = api.read("GET", "/api/teams/nobody", Some(&carol)).await;
    assert_eq!((status, missing_status), (404, 404));
    assert_eq!(hidden, missing);
    assert_eq!(code(&hidden), "flare::teams::not_found");
    let (status, listed) = api.read("GET", "/api/teams", Some(&bob)).await;
    assert_eq!(status, 200);
    assert_eq!(listed["items"][0]["name"], "design");
}

#[tokio::test]
async fn only_the_owner_adds_members_and_a_member_leaves() {
    let (api, _alice, bob, _carol) = with_design().await;
    let (status, refused) = api
        .json(
            "POST",
            "/api/teams/design/members",
            Some(&bob),
            json!({"with": ["carol@example.com"]}),
        )
        .await;
    assert_eq!(status, 403);
    assert_eq!(code(&refused), "flare::teams::forbidden");
    let (status, left) = api
        .json(
            "DELETE",
            "/api/teams/design/members",
            Some(&bob),
            json!({"with": ["bob@example.com"]}),
        )
        .await;
    assert_eq!(status, 200, "{left}");
    assert_eq!(left["changed"], json!(["bob@example.com"]));
    assert_eq!(left["team"]["members"], 1);
}

#[tokio::test]
async fn the_owner_leaving_their_own_team_answers_owner_cannot_leave() {
    let (api, alice, _bob, _carol) = with_design().await;
    let (status, refused) = api
        .json(
            "DELETE",
            "/api/teams/design/members",
            Some(&alice),
            json!({"with": ["alice@example.com"]}),
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(code(&refused), "flare::teams::owner_cannot_leave");
}

#[tokio::test]
async fn a_share_with_a_team_opens_to_its_members_until_one_is_removed() {
    let (api, alice, bob, carol) = with_design().await;
    let id = api.share(&alice, "Plan", &["team:design"], &[]).await;
    let path = format!("/api/shares/{id}");
    assert_eq!(api.read("GET", &path, Some(&bob)).await.0, 200);
    assert_eq!(api.read("GET", &path, Some(&carol)).await.0, 404);
    let (status, _) = api
        .json(
            "DELETE",
            "/api/teams/design/members",
            Some(&alice),
            json!({"with": ["bob@example.com"]}),
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(api.read("GET", &path, Some(&bob)).await.0, 404);
}

#[tokio::test]
async fn naming_a_team_the_caller_is_not_in_answers_team_not_found_and_writes_nothing() {
    let (api, _alice, _bob, carol) = with_design().await;
    let (status, refused) = api
        .json(
            "POST",
            "/api/shares",
            Some(&carol),
            share_body("Plan", &["team:design"], &[]),
        )
        .await;
    assert_eq!(status, 404);
    assert_eq!(code(&refused), "flare::teams::not_found");
    let (_, mine) = api
        .read("GET", "/api/shares?scope=mine", Some(&carol))
        .await;
    assert_eq!(mine["items"], json!([]));
}

#[tokio::test]
async fn deleting_a_team_removes_its_grants_and_a_new_team_of_the_name_opens_nothing_old() {
    let (api, alice, bob, carol) = with_design().await;
    let id = api.share(&alice, "Plan", &["team:design"], &[]).await;
    let (status, deleted) = api.read("DELETE", "/api/teams/design", Some(&alice)).await;
    assert_eq!(status, 200, "{deleted}");
    assert_eq!(
        deleted,
        json!({"team": "design", "finished": true, "grants_removed": 1})
    );
    assert_eq!(
        api.read("GET", &format!("/api/shares/{id}"), Some(&bob))
            .await
            .0,
        404
    );
    let grants = format!("/api/grants/{id}");
    let (status, listed) = api.read("GET", &grants, Some(&alice)).await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(listed["items"], json!([]));
    let (status, again) = api
        .json(
            "POST",
            "/api/teams",
            Some(&carol),
            json!({"name": "design"}),
        )
        .await;
    assert_eq!(status, 201, "{again}");
    assert_eq!(again["owner"], "carol@example.com");
    assert_eq!(
        api.read("GET", &format!("/api/shares/{id}"), Some(&carol))
            .await
            .0,
        404
    );
}

#[tokio::test]
async fn the_owner_transfers_a_team_to_a_member_and_a_stranger_is_refused() {
    let (api, alice, bob, _carol) = with_design().await;
    let (status, refused) = api
        .json(
            "PUT",
            "/api/teams/design/owner",
            Some(&alice),
            json!({"to": "carol@example.com"}),
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(code(&refused), "flare::teams::not_a_member");
    let (status, by_member) = api
        .json(
            "PUT",
            "/api/teams/design/owner",
            Some(&bob),
            json!({"to": "bob@example.com"}),
        )
        .await;
    assert_eq!(status, 403, "{by_member}");
    assert_eq!(code(&by_member), "flare::teams::forbidden");
    let (status, moved) = api
        .json(
            "PUT",
            "/api/teams/design/owner",
            Some(&alice),
            json!({"to": "bob@example.com"}),
        )
        .await;
    assert_eq!(status, 200, "{moved}");
    assert_eq!(moved["owner"], "bob@example.com");
    assert_eq!(moved["role"], "member");
    assert_eq!(moved["members"], 2);
    let (status, shown) = api.read("GET", "/api/teams/design", Some(&bob)).await;
    assert_eq!(status, 200, "{shown}");
    assert_eq!(shown["team"]["role"], "owner");
}

#[tokio::test]
async fn an_everyone_grant_opens_to_the_domain_and_a_reader_cannot_add_it() {
    let (api, alice, bob, carol) = with_design().await;
    let id = api.share(&alice, "Note", &["bob@example.com"], &[]).await;
    let grants = format!("/api/grants/{id}");
    let (status, granted) = api
        .json("POST", &grants, Some(&alice), json!({"everyone": true}))
        .await;
    assert_eq!(status, 200, "{granted}");
    assert_eq!(granted["granted"], json!(["everyone"]));
    let path = format!("/api/shares/{id}");
    assert_eq!(api.read("GET", &path, Some(&carol)).await.0, 200);
    let (_, listed) = api.read("GET", &grants, Some(&alice)).await;
    let everyone = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["kind"] == "everyone")
        .unwrap()
        .clone();
    assert_eq!(everyone["audience"], "example.com");
    let (status, refused) = api
        .json("POST", &grants, Some(&bob), json!({"everyone": true}))
        .await;
    assert_eq!(status, 403, "a reader may not grant: {refused}");
    let (status, removed) = api
        .json("DELETE", &grants, Some(&alice), json!({"everyone": true}))
        .await;
    assert_eq!(status, 200, "{removed}");
    assert_eq!(api.read("GET", &path, Some(&carol)).await.0, 404);
}

#[tokio::test]
async fn shares_with_me_leaves_out_everyone_shares_unless_asked() {
    let (api, alice, bob, _carol) = with_design().await;
    let team_share = api.share(&alice, "Team", &["team:design"], &[]).await;
    let open_share = api.share(&alice, "Open", &["everyone"], &[]).await;
    let (_, plain) = api
        .read("GET", "/api/shares?scope=with-me", Some(&bob))
        .await;
    let ids: Vec<&str> = plain["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![team_share.as_str()]);
    let (status, all) = api
        .read("GET", "/api/shares?scope=with-me&everyone=true", Some(&bob))
        .await;
    assert_eq!(status, 200, "{all}");
    assert_eq!(all["items"].as_array().unwrap().len(), 2);
    assert!(
        all["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == open_share.as_str())
    );
}

#[tokio::test]
async fn under_an_anyone_rule_an_everyone_grant_is_refused() {
    let api = Api::start().await;
    let alice = api.token_for("alice").await;
    let (status, refused) = api
        .json(
            "POST",
            "/api/shares",
            Some(&alice),
            share_body("Note", &["everyone"], &[]),
        )
        .await;
    assert_eq!(status, 409, "{refused}");
    assert_eq!(code(&refused), "flare::grants::everyone_unbounded");
}

#[tokio::test]
async fn with_everyone_signed_in_on_an_everyone_grant_under_an_anyone_rule_opens_to_every_signed_in_person()
 {
    let api = Api::start_everyone_signed_in().await;
    let alice = api.token_for("alice").await;
    let dave = api.token_for("dave").await;
    let id = api.share(&alice, "Note", &["everyone"], &[]).await;
    let path = format!("/api/shares/{id}");
    let (status, opened) = api.read("GET", &path, Some(&dave)).await;
    assert_eq!(status, 200, "{opened}");
    let (status, grants) = api
        .read("GET", &format!("/api/grants/{id}"), Some(&alice))
        .await;
    assert_eq!(status, 200, "{grants}");
    assert!(grants.to_string().contains("everyone"), "{grants}");
    // The twin: a share named to no one stays closed to dave.
    let closed = api.share(&alice, "Private", &[], &[]).await;
    let path = format!("/api/shares/{closed}");
    assert_eq!(api.read("GET", &path, Some(&dave)).await.0, 404);
}

#[tokio::test]
async fn revoke_removes_a_persons_grant_and_an_empty_body_is_refused() {
    let (api, alice, bob, _carol) = with_design().await;
    let id = api.share(&alice, "Plan", &["carol@example.com"], &[]).await;
    let grants = format!("/api/grants/{id}");
    let (status, revoked) = api
        .json(
            "DELETE",
            &grants,
            Some(&alice),
            json!({"with": ["carol@example.com"]}),
        )
        .await;
    assert_eq!(status, 200, "{revoked}");
    assert_eq!(revoked["revoked"], json!(["carol@example.com"]));
    let (status, empty) = api.json("DELETE", &grants, Some(&alice), json!({})).await;
    assert_eq!(status, 400);
    assert_eq!(code(&empty), "flare::requests::bad_body");
    let (status, _) = api
        .json(
            "DELETE",
            &grants,
            Some(&bob),
            json!({"with": ["carol@example.com"]}),
        )
        .await;
    assert_eq!(status, 404);
}

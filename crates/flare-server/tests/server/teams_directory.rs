//! The team routes and team grants on a hosted site whose teams come from
//! the deployment's own teams document: what a member sees, what every
//! change answers, and how a share to a team follows the document as it is
//! read again.

use crate::api::Api;
use flare_core::{
    DOCUMENT_MAX_AGE, DocumentTeams, DocumentUnreadable, IdentityKind, TeamsDocument,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Instant;

fn code(body: &Value) -> &str {
    body["code"].as_str().unwrap_or_default()
}

/// The document that the deployment's sync job writes.
#[derive(Clone)]
struct Sync(Arc<Mutex<Value>>);

impl Sync {
    fn write(&self, document: Value) {
        *self.0.lock().unwrap() = document;
    }
}

impl TeamsDocument for Sync {
    async fn read(&self) -> Result<Vec<u8>, DocumentUnreadable> {
        Ok(serde_json::to_vec(&*self.0.lock().unwrap()).unwrap())
    }
}

/// The clock that ages the copy of the document.
#[derive(Clone)]
struct Clock(Arc<Mutex<Instant>>);

impl Clock {
    fn pass_max_age(&self) {
        *self.0.lock().unwrap() += DOCUMENT_MAX_AGE;
    }
}

fn design_with(members: &[&str]) -> Value {
    let members: Vec<String> = members
        .iter()
        .map(|name| format!("{name}@example.com"))
        .collect();
    json!({"teams": {"design": members}})
}

/// A site whose document puts alice and bob in design. Answers the site,
/// the document, its clock and the tokens of alice, bob and carol.
async fn directory_site() -> (Api, Sync, Clock, [String; 3]) {
    let sync = Sync(Arc::new(Mutex::new(design_with(&["alice", "bob"]))));
    let clock = Clock(Arc::new(Mutex::new(Instant::now())));
    let ticks = Arc::clone(&clock.0);
    let teams = DocumentTeams::load(sync.clone(), IdentityKind::Email)
        .await
        .unwrap()
        .with_clock(move || *ticks.lock().unwrap());
    let api = Api::start_with_teams(teams).await;
    let tokens = [
        api.token_for("alice").await,
        api.token_for("bob").await,
        api.token_for("carol").await,
    ];
    (api, sync, clock, tokens)
}

#[tokio::test]
async fn a_member_lists_a_directory_team_with_no_owner_and_sees_its_members() {
    let (api, _, _, [alice, bob, carol]) = directory_site().await;
    let (status, listed) = api.read("GET", "/api/teams", Some(&alice)).await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(
        listed["items"],
        json!([{"name": "design", "managed_by": "directory", "role": "member", "members": 2}])
    );
    let (status, shown) = api.read("GET", "/api/teams/design", Some(&bob)).await;
    assert_eq!(status, 200, "{shown}");
    assert_eq!(
        shown["members"],
        json!(["alice@example.com", "bob@example.com"])
    );
    let (status, hidden) = api.read("GET", "/api/teams/design", Some(&carol)).await;
    assert_eq!(status, 404, "{hidden}");
    assert_eq!(code(&hidden), "flare::teams::not_found");
    let (_, none) = api.read("GET", "/api/teams", Some(&carol)).await;
    assert_eq!(none["items"], json!([]));
}

#[tokio::test]
async fn every_team_change_answers_managed_elsewhere_and_the_team_stays_as_the_document_says() {
    let (api, _, _, [alice, _bob, carol]) = directory_site().await;
    let changes = [
        ("POST", "/api/teams", json!({"name": "ops"})),
        (
            "POST",
            "/api/teams/design/members",
            json!({"with": ["carol@example.com"]}),
        ),
        (
            "DELETE",
            "/api/teams/design/members",
            json!({"with": ["bob@example.com"]}),
        ),
        (
            "PUT",
            "/api/teams/design/owner",
            json!({"to": "bob@example.com"}),
        ),
    ];
    for token in [&alice, &carol] {
        for (method, path, body) in &changes {
            let (status, refused) = api.json(method, path, Some(token), body.clone()).await;
            assert_eq!(status, 403, "{method} {path}: {refused}");
            assert_eq!(code(&refused), "flare::teams::managed_elsewhere");
        }
        let (status, refused) = api.read("DELETE", "/api/teams/design", Some(token)).await;
        assert_eq!(status, 403, "{refused}");
        assert_eq!(code(&refused), "flare::teams::managed_elsewhere");
    }
    let (_, shown) = api.read("GET", "/api/teams/design", Some(&alice)).await;
    assert_eq!(shown["team"]["members"], 2);
    let (_, ops) = api.read("GET", "/api/teams/ops", Some(&alice)).await;
    assert_eq!(code(&ops), "flare::teams::not_found");
}

#[tokio::test]
async fn a_share_with_a_directory_team_opens_to_listed_members_until_a_reread_drops_one() {
    let (api, sync, clock, [alice, bob, carol]) = directory_site().await;
    let id = api.share(&alice, "Plan", &["team:design"], &[]).await;
    let path = format!("/api/shares/{id}");
    assert_eq!(api.read("GET", &path, Some(&bob)).await.0, 200);
    assert_eq!(api.read("GET", &path, Some(&carol)).await.0, 404);
    sync.write(design_with(&["alice"]));
    assert_eq!(api.read("GET", &path, Some(&bob)).await.0, 200);
    clock.pass_max_age();
    assert_eq!(api.read("GET", &path, Some(&bob)).await.0, 404);
}

#[tokio::test]
async fn a_team_that_leaves_the_document_has_dormant_grants_that_open_again_when_it_returns() {
    let (api, sync, clock, [alice, bob, _carol]) = directory_site().await;
    let id = api.share(&alice, "Plan", &["team:design"], &[]).await;
    let grants = format!("/api/grants/{id}");
    let state = |listed: &Value| -> Value {
        listed["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|grant| grant["kind"] == "team")
            .map(|grant| grant["state"].clone())
            .unwrap_or(Value::Null)
    };
    sync.write(json!({"teams": {}}));
    clock.pass_max_age();
    let (status, listed) = api.read("GET", &grants, Some(&alice)).await;
    assert_eq!(status, 200, "{listed}");
    assert_eq!(state(&listed), "dormant", "{listed}");
    let path = format!("/api/shares/{id}");
    assert_eq!(api.read("GET", &path, Some(&bob)).await.0, 404);
    sync.write(design_with(&["alice", "bob"]));
    clock.pass_max_age();
    let (_, listed) = api.read("GET", &grants, Some(&alice)).await;
    assert_eq!(state(&listed), "active", "{listed}");
    assert_eq!(api.read("GET", &path, Some(&bob)).await.0, 200);
}

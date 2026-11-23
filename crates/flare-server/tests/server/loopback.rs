//! Plan test 13: the probe, the code route, the opener and the launch
//! document.

use crate::harness::{Options, Server, cookie_pair, limits};
use crate::scratch::mode;
use flare_auth_loopback::Nonce;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[tokio::test]
async fn the_probe_answers_the_proof_that_flare_open_checks() {
    let server = Server::start().await;
    let nonce = Nonce::new().unwrap();
    let reply = server
        .get(&format!("/launch/probe?nonce={}", nonce.as_str()))
        .send()
        .await;
    assert_eq!(reply.status, 200);
    assert!(
        server
            .loopback
            .server_record()
            .proof_matches(&nonce, reply.body.trim())
    );
    let malformed = server.get("/launch/probe?nonce=").send().await;
    assert_eq!(malformed.status, 400);
}

async fn ask_for_code(server: &Server, return_to: &str) -> PathBuf {
    let reply = server
        .request("POST", &format!("/launch/codes?return_to={return_to}"))
        .bearer(&server.token())
        .send()
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.header("content-type"), Some("application/json"));
    let answer: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    PathBuf::from(answer["opener"].as_str().unwrap())
}

fn launch_path(opener: &PathBuf) -> String {
    let page = std::fs::read_to_string(opener).unwrap();
    let start = page.find("/launch?code=").unwrap();
    let end = page[start..].find('"').unwrap() + start;
    page[start..end].to_owned()
}

#[tokio::test]
async fn the_code_route_writes_an_opener_of_mode_0600_for_a_bearer_token() {
    let server = Server::start().await;
    let opener = ask_for_code(&server, "/flare/").await;
    assert!(opener.starts_with(server.state.path()));
    assert_eq!(mode(&opener), 0o600);
    assert!(launch_path(&opener).starts_with("/launch?code="));
}

#[tokio::test]
async fn the_code_route_refuses_a_session_cookie() {
    let server = Server::start().await;
    let cookie = server.sign_in().await;
    let reply = server
        .request("POST", "/launch/codes")
        .cookie(&cookie)
        .same_origin()
        .send()
        .await;
    assert_eq!(reply.status, 403);
    assert_eq!(reply.code().as_deref(), Some("launch::needs_bearer"));
}

#[tokio::test]
async fn the_launch_document_sets_the_session_cookie_refreshes_to_the_target_and_removes_the_opener()
 {
    let server = Server::start().await;
    let opener = ask_for_code(&server, "/pages/home").await;
    let reply = server.get(&launch_path(&opener)).send().await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let cookie = reply.all("set-cookie");
    assert_eq!(cookie.len(), 1);
    assert!(cookie[0].starts_with("flare_session="));
    assert!(cookie[0].contains("HttpOnly") && cookie[0].contains("SameSite=Strict"));
    assert!(
        reply
            .body
            .contains("<meta http-equiv=\"refresh\" content=\"0;url=/pages/home\">")
    );
    assert!(reply.body.contains("href=\"/pages/home\""));
    assert!(!opener.exists(), "the opener is removed after sign-in");
    let session = cookie_pair(&reply, "flare_session").unwrap();
    assert_eq!(
        server.get("/flare/").cookie(&session).send().await.status,
        200
    );
    let unknown = server
        .get(&format!("/launch?code={}", "0".repeat(64)))
        .send()
        .await;
    assert_eq!(unknown.status, 403);
    assert_eq!(unknown.code().as_deref(), Some("launch::code_refused"));
}

#[tokio::test]
async fn a_used_code_is_refused() {
    let server = Server::start().await;
    let opener = ask_for_code(&server, "/flare/").await;
    let path = launch_path(&opener);
    assert_eq!(server.get(&path).send().await.status, 200);
    let used = server.get(&path).send().await;
    assert_eq!(
        (used.status, used.code().as_deref()),
        (403, Some("launch::code_refused"))
    );
}

#[tokio::test]
async fn an_opener_is_removed_once_its_code_expires() {
    let server = Server::start_with(Options {
        limits: Some(limits(|limits| {
            limits.sweep_every = Duration::from_millis(50)
        })),
        ..Options::default()
    })
    .await;
    let opener = ask_for_code(&server, "/flare/").await;
    let path = launch_path(&opener);
    server.time.advance(Duration::from_secs(61));
    let deadline = Instant::now() + Duration::from_secs(10);
    while opener.exists() {
        assert!(Instant::now() < deadline, "the opener outlived its code");
        tokio::task::yield_now().await;
    }
    assert_eq!(server.get(&path).send().await.status, 403);
}

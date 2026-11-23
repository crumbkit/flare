//! The lifecycle's crash points 3, 5, 7, 8 and 9, and the request deadline
//! with a write in work. A later server on the same board shares the
//! earlier one's stores and state directory, as a new `flare serve`
//! process does. Crash point 2 needs a killed process, so it is in
//! hangar-cli's tests/serve.rs.

use crate::fake_provider::connect;
use crate::forms::{draw, post};
use crate::harness::{BINDING, BODY, Options, RECORD, Server, exchange, limits, set};
use crate::oidc::{callback, finish_path, oidc_server, start};
use hangar_server::Stopped;
use std::os::unix::fs::PermissionsExt as _;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::sync::Semaphore;

fn in_work(server: &Server) -> usize {
    server.switches.writes_in_work.load(Ordering::SeqCst)
}

/// Holds every later record write until the test adds a permit.
fn hold_writes(server: &Server) -> Arc<Semaphore> {
    let gate = Arc::new(Semaphore::new(0));
    *server.switches.write_hold.lock().unwrap() = Some(Arc::clone(&gate));
    gate
}

#[tokio::test]
async fn a_write_past_the_request_deadline_is_stopped_before_the_503_answers() {
    let server = Server::start_with(Options {
        limits: Some(limits(|limits| {
            limits.request_time = Duration::from_millis(300);
        })),
        ..Options::default()
    })
    .await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &cookie).await;
    set(&mut pairs, "f.title", "Too slow to save");
    let gate = hold_writes(&server);
    let reply = post(&server, BINDING, &cookie, &pairs).await;
    assert_eq!(reply.status, 503, "{}", reply.body);
    assert_eq!(reply.code().as_deref(), Some("server::deadline"));
    assert_eq!(in_work(&server), 0, "the write still ran after the 503");
    gate.add_permits(8);
    assert_eq!(server.body_of(RECORD).await, BODY);
    assert_eq!(server.switches.record_writes.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn crash_point_7_a_drain_past_its_limit_stops_a_write_in_work_before_serve_returns() {
    let mut server = Server::start_with(Options {
        limits: Some(limits(|limits| limits.drain = Duration::from_millis(200))),
        ..Options::default()
    })
    .await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let mut pairs = draw(&server, BINDING, &cookie).await;
    set(&mut pairs, "f.title", "Cut off by the drain");
    let gate = hold_writes(&server);
    let raw = server
        .request("POST", &format!("/b/{BINDING}"))
        .cookie(&cookie)
        .same_origin()
        .form(&pairs)
        .bytes();
    let port = server.port;
    #[expect(
        clippy::disallowed_methods,
        reason = "the test awaits the request's task"
    )]
    let request = tokio::spawn(async move { exchange(port, &raw).await });
    server.switches.write_held.notified().await;
    assert_eq!(in_work(&server), 1);
    assert_eq!(server.stop().await, Stopped::Shutdown);
    assert_eq!(in_work(&server), 0, "a write outlived serve");
    gate.add_permits(8);
    assert!(
        request.await.is_err(),
        "the connection closes with no reply"
    );
    assert_eq!(server.body_of(RECORD).await, BODY);
    assert!(server.state.read_server().unwrap().is_none());
}

#[tokio::test]
async fn crash_point_3_a_post_from_a_form_an_earlier_server_drew_is_refused() {
    let mut first = Server::start().await;
    first.seed_binding().await;
    let cookie = first.sign_in().await;
    let mut pairs = draw(&first, BINDING, &cookie).await;
    set(&mut pairs, "f.title", "Saved before the crash");
    assert_eq!(post(&first, BINDING, &cookie, &pairs).await.status, 303);
    first.stop().await;
    let second = Server::start_with(Options {
        after: Some(first.earlier()),
        ..Options::default()
    })
    .await;
    let cookie = second.sign_in().await;
    let resubmitted = post(&second, BINDING, &cookie, &pairs).await;
    assert_eq!(resubmitted.status, 403, "{}", resubmitted.body);
    assert_eq!(resubmitted.code().as_deref(), Some("forms::token_unknown"));
    assert_eq!(second.switches.record_writes.load(Ordering::SeqCst), 1);
    let reloaded = second
        .get(&format!("/b/{BINDING}"))
        .cookie(&cookie)
        .send()
        .await;
    assert!(
        reloaded.body.contains("Saved before the crash"),
        "{}",
        reloaded.body
    );
}

#[tokio::test]
async fn crash_point_5_a_hand_over_code_from_an_earlier_server_is_unknown() {
    let (mut first, fake, _clock) = oidc_server().await;
    let (_, location, kept) = start(&first).await;
    let reply = callback(&first, &fake.approve(&location), &kept).await;
    assert_eq!(reply.status, 303, "{}", reply.body);
    let finish = finish_path(&first, &reply);
    first.stop().await;
    let (authenticator, _clock) = connect(&fake).await;
    let second = Server::start_with(Options {
        oidc: Some(authenticator),
        after: Some(first.earlier()),
        ..Options::default()
    })
    .await;
    let refused = second.get(&finish).send().await;
    assert_eq!(refused.status, 403, "{}", refused.body);
    assert_eq!(refused.code().as_deref(), Some("oidc::handover_unknown"));
    assert!(refused.header("set-cookie").is_none());
}

#[tokio::test]
async fn crash_point_8_a_server_file_that_could_not_be_removed_is_replaced_by_the_next_server() {
    let mut first = Server::start().await;
    let directory = first.state.path().to_owned();
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o500)).unwrap();
    let stopped = first.stop().await;
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(stopped, Stopped::Shutdown);
    assert!(first.named_by_server_file(), "the file is left stale");
    let mut second = Server::start_with(Options {
        after: Some(first.earlier()),
        ..Options::default()
    })
    .await;
    assert!(second.named_by_server_file());
    assert_eq!(second.get("/up").send().await.status, 200);
    second.stop().await;
    assert!(second.state.read_server().unwrap().is_none());
}

#[tokio::test]
async fn crash_point_9_two_servers_on_one_board_both_serve_and_the_later_file_wins() {
    let mut first = Server::start().await;
    let first_cookie = first.sign_in().await;
    let mut second = Server::start_with(Options {
        after: Some(first.earlier()),
        ..Options::default()
    })
    .await;
    assert!(second.named_by_server_file());
    assert!(!first.named_by_server_file());
    let signed_in = first.get("/flare/").cookie(&first_cookie).send().await;
    assert_eq!(signed_in.status, 200, "{}", signed_in.body);
    first.stop().await;
    assert!(
        second.named_by_server_file(),
        "a server removes only its own record"
    );
    second.stop().await;
    assert!(second.state.read_server().unwrap().is_none());
}

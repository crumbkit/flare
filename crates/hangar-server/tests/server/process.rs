//! Plan tests 14 and 15: shutdown, the idle exit, `/up` and `/ready`, a
//! panicking handler, and that no secret reaches the log or a `Debug`
//! output.

use crate::forms::draw;
use crate::harness::{BINDING, Options, PAGE, Server, exchange, limits, value};
use hangar_server::Stopped;
use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Semaphore;

#[tokio::test]
async fn shutdown_lets_a_slow_request_finish_and_removes_the_server_file() {
    let mut server = Server::start().await;
    server.seed_page().await;
    let cookie = server.sign_in().await;
    let gate = Arc::new(Semaphore::new(0));
    *server.switches.hold.lock().unwrap() = Some(Arc::clone(&gate));
    let slow = server
        .get(&format!("/pages/{PAGE}"))
        .cookie(&cookie)
        .bytes();
    let port = server.port;
    let request = tokio::spawn(async move { exchange(port, &slow).await });
    server.switches.held.notified().await;
    server.signal_shutdown();
    gate.add_permits(1);
    assert_eq!(request.await.unwrap().status, 200);
    assert_eq!(server.join().await, Stopped::Shutdown);
    assert!(
        server.state.read_server().unwrap().is_none(),
        "server.json is removed"
    );
    assert!(
        tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn the_server_exits_after_its_idle_period_and_removes_the_server_file() {
    let mut server = Server::start_with(Options {
        limits: Some(limits(|limits| {
            limits.idle_exit = Duration::from_millis(400)
        })),
        ..Options::default()
    })
    .await;
    assert_eq!(server.get("/up").send().await.status, 200);
    assert_eq!(server.join().await, Stopped::Idle);
    assert!(server.state.read_server().unwrap().is_none());
}

#[tokio::test]
async fn up_answers_200_and_ready_answers_503_while_the_board_is_unavailable() {
    let server = Server::start().await;
    let up = server.get("/up").send().await;
    assert_eq!((up.status, up.body.as_str()), (200, ""));
    assert_eq!(server.get("/ready").send().await.status, 200);
    server.switches.pages_down.store(true, Ordering::SeqCst);
    let down = server.get("/ready").send().await;
    assert_eq!(
        (down.status, down.code().as_deref()),
        (503, Some("board::unavailable"))
    );
    assert_eq!(server.get("/up").send().await.status, 200);
    server.switches.pages_down.store(false, Ordering::SeqCst);
    assert_eq!(server.get("/ready").send().await.status, 200);
}

#[tokio::test]
async fn a_panicking_handler_answers_500_and_the_server_keeps_serving() {
    let server = Server::start().await;
    server.seed_page().await;
    let cookie = server.sign_in().await;
    server.switches.panic_on_read.store(true, Ordering::SeqCst);
    let reply = server
        .get(&format!("/pages/{PAGE}"))
        .cookie(&cookie)
        .send()
        .await;
    assert_eq!(
        (reply.status, reply.code().as_deref()),
        (500, Some("server::fault"))
    );
    let again = server
        .get(&format!("/pages/{PAGE}"))
        .cookie(&cookie)
        .send()
        .await;
    assert_eq!(again.status, 200);
}

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "current_thread")]
async fn no_secret_reaches_the_log_or_a_debug_output() {
    let log = Captured::default();
    let writer = log.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let server = Server::start().await;
    server.seed_binding().await;
    let token = server.token();
    let cookie = server.sign_in().await;
    let session = cookie.trim_start_matches("hangar_session=").to_owned();
    let pairs = draw(&server, BINDING, &cookie).await;
    let form_token = value(&pairs, "token").to_owned();
    let codes = server
        .request("POST", "/launch/codes?return_to=/hangar/")
        .bearer(&token)
        .send()
        .await;
    assert_eq!(codes.status, 200);
    server
        .get("/launch?code=a-wrong-code-that-is-not-logged")
        .send()
        .await;
    let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    assert!(
        text.contains("status=200"),
        "the request log is written: {text}"
    );
    for secret in [
        token.as_str(),
        session.as_str(),
        form_token.as_str(),
        "a-wrong-code-that-is-not-logged",
    ] {
        assert!(!text.contains(secret), "the log holds a secret: {text}");
        assert!(
            !server.debug.contains(secret),
            "Debug holds a secret: {}",
            server.debug
        );
        assert!(!format!("{:?}", server.loopback).contains(secret));
    }
    assert!(!text.contains("code="), "the log holds a query string");
    assert!(
        !server.debug.contains(&server.authority),
        "Debug holds the launch host"
    );
    assert!(
        !text.contains(&server.authority),
        "the log holds the launch host"
    );
}

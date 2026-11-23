//! Plan test 14: shutdown, the idle exit, `/up` and `/ready`, and a
//! panicking handler. Plan test 15, that no secret reaches the log, is in
//! tests/log.rs, because it needs a test binary of its own.

use crate::harness::{Options, PAGE, Server, exchange, limits};
use flare_server::Stopped;
use std::sync::Arc;
use std::sync::atomic::Ordering;
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
    #[expect(
        clippy::disallowed_methods,
        reason = "the test awaits the request's task"
    )]
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

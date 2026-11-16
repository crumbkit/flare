//! Plan test 7 (threat 21): each limit answers its code, and the next
//! request is served.

use crate::harness::{Options, PAGE, Server, exchange, limits, parse};
use hangar_server::Limits;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;

async fn with_limits(edit: impl FnOnce(&mut Limits)) -> Server {
    Server::start_with(Options {
        limits: Some(limits(edit)),
        ..Options::default()
    })
    .await
}

async fn assert_next_request_served(server: &Server) {
    let reply = server.get("/up").send().await;
    assert_eq!(reply.status, 200, "the next request");
}

#[tokio::test]
async fn a_header_block_over_32_kib_answers_431_and_the_next_request_is_served() {
    let server = Server::start().await;
    let just_under = server
        .get("/up")
        .header("X-Filler", &"a".repeat(30 * 1024))
        .send()
        .await;
    assert_eq!(just_under.status, 200);
    let over = server
        .get("/up")
        .header("X-Filler", &"a".repeat(33 * 1024))
        .send()
        .await;
    assert_eq!(over.status, 431);
    assert_eq!(over.code().as_deref(), Some("request::headers_too_large"));
    assert_next_request_served(&server).await;
}

#[tokio::test]
async fn a_body_over_1_mib_answers_413_and_the_next_request_is_served() {
    let server = Server::start().await;
    server.seed_binding().await;
    let cookie = server.sign_in().await;
    let reply = server
        .request("POST", "/b/kitchen")
        .cookie(&cookie)
        .same_origin()
        .body(
            "application/x-www-form-urlencoded",
            vec![b'a'; 1024 * 1024 + 1],
        )
        .send()
        .await;
    assert_eq!(reply.status, 413);
    assert_eq!(reply.code().as_deref(), Some("request::body_too_large"));
    assert_next_request_served(&server).await;
}

#[tokio::test]
async fn a_body_that_arrives_too_slowly_answers_408_and_the_next_request_is_served() {
    let server = with_limits(|limits| limits.read_time = Duration::from_millis(300)).await;
    let cookie = server.sign_in().await;
    let call = server
        .request("POST", "/b/kitchen")
        .cookie(&cookie)
        .same_origin()
        .body("application/x-www-form-urlencoded", Vec::new());
    let mut head = String::from_utf8(call.bytes()).unwrap();
    head = head.replace("Content-Length: 0", "Content-Length: 100");
    let mut stream = TcpStream::connect(("127.0.0.1", server.port))
        .await
        .unwrap();
    stream.write_all(head.as_bytes()).await.unwrap();
    stream.write_all(b"token=abc").await.unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut raw))
        .await
        .unwrap()
        .unwrap();
    let reply = parse(&raw);
    assert_eq!(reply.status, 408);
    assert_eq!(reply.code().as_deref(), Some("request::slow"));
    assert_next_request_served(&server).await;
}

#[tokio::test]
async fn a_header_block_that_arrives_too_slowly_loses_its_connection_and_the_next_request_is_served()
 {
    let server = with_limits(|limits| limits.read_time = Duration::from_millis(300)).await;
    let mut stream = TcpStream::connect(("127.0.0.1", server.port))
        .await
        .unwrap();
    stream
        .write_all(b"GET /up HTTP/1.1\r\nHost: ")
        .await
        .unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut raw))
        .await
        .expect("the server closes the connection")
        .unwrap();
    assert_next_request_served(&server).await;
}

#[tokio::test]
async fn a_request_past_the_in_work_limit_answers_503_and_the_next_request_is_served() {
    let server = with_limits(|limits| limits.in_work = 1).await;
    server.seed_page().await;
    let cookie = server.sign_in().await;
    let gate = Arc::new(Semaphore::new(0));
    *server.switches.hold.lock().unwrap() = Some(Arc::clone(&gate));
    let held = server
        .get(&format!("/pages/{PAGE}"))
        .cookie(&cookie)
        .bytes();
    let port = server.port;
    let first = tokio::spawn(async move { exchange(port, &held).await });
    server.switches.held.notified().await;
    let busy = server.get("/up").send().await;
    assert_eq!(busy.status, 503);
    assert_eq!(busy.code().as_deref(), Some("server::busy"));
    gate.add_permits(1);
    assert_eq!(first.await.unwrap().status, 200);
    assert_next_request_served(&server).await;
}

#[tokio::test]
async fn a_request_past_its_deadline_answers_503_and_the_next_request_is_served() {
    let server = with_limits(|limits| limits.request_time = Duration::from_millis(300)).await;
    server.seed_page().await;
    let cookie = server.sign_in().await;
    let gate = Arc::new(Semaphore::new(0));
    *server.switches.hold.lock().unwrap() = Some(Arc::clone(&gate));
    let reply = server
        .get(&format!("/pages/{PAGE}"))
        .cookie(&cookie)
        .send()
        .await;
    assert_eq!(reply.status, 503);
    assert_eq!(reply.code().as_deref(), Some("server::deadline"));
    *server.switches.hold.lock().unwrap() = None;
    assert_next_request_served(&server).await;
    let page = server
        .get(&format!("/pages/{PAGE}"))
        .cookie(&cookie)
        .send()
        .await;
    assert_eq!(page.status, 200);
}

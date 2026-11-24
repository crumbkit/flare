//! Plan test 16, the client: `flare open` fails with `server::not_running`
//! when nothing answers, and sends no token to a server that gives a wrong
//! proof.
#![cfg(test)]

#[path = "../../flare-auth-loopback/tests/support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch directory is flare-auth-loopback's test support, and these tests use part of it"
)]
pub mod loopback_scratch;
#[path = "support/run.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the run helper is test support, and each test file uses part of it"
)]
pub mod run;

use flare_auth_loopback::{Loopback, Served, StateDir, local_principal};
use run::Home;
use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// A board directory for `--board`. `open` reads only the state directory
/// of the board, so the directory needs no board in it.
fn board_dir(home: &Home) -> std::path::PathBuf {
    let path = home.path().join("board");
    std::fs::create_dir(&path).unwrap();
    std::fs::canonicalize(path).unwrap()
}

/// Writes a server file for a server on `port` and returns the loopback
/// state it was made from.
fn write_server_file(home: &Home, board: &Path, port: u16) -> Loopback {
    let loopback =
        Loopback::new(port, local_principal("tester").unwrap(), Served::Everything).unwrap();
    let state = StateDir::for_board(&home.state(), board);
    state.prepare(SystemTime::now()).unwrap();
    state.write_server(&loopback.server_record()).unwrap();
    loopback
}

/// A server that answers every request with `proof` as its body and keeps
/// each request's head.
fn fake_server(proof: &'static str) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let heads = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&heads);
    #[expect(
        clippy::disallowed_methods,
        reason = "the fake server's thread answers until the test process ends"
    )]
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut bytes = [0_u8; 8192];
            let read = stream.read(&mut bytes).unwrap_or(0);
            kept.lock()
                .unwrap()
                .push(String::from_utf8_lossy(&bytes[..read]).into_owned());
            let reply = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{proof}",
                proof.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    (port, heads)
}

#[test]
fn a_closed_standard_output_stops_the_command_without_a_panic() {
    let home = Home::new();
    #[expect(
        clippy::disallowed_methods,
        reason = "the test starts the built binary with its output closed"
    )]
    let mut child = home
        .command(&["commands", "list", "--json", "--full"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("panicked"), "{stderr}");
    assert_ne!(output.status.code(), Some(101), "{stderr}");
}

#[test]
fn open_fails_with_server_not_running_when_there_is_no_server_file() {
    let home = Home::new();
    let board = board_dir(&home);
    let board = board.to_str().unwrap();
    let ran = home.run(&["open", "--board", board, "--json"]);
    ran.assert_failed("flare::server::not_running");
    let printed: serde_json::Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        printed["recovery"],
        serde_json::json!({"kind": "run", "call": format!("flare serve --board {board}")}),
        "{}",
        ran.all()
    );
}

#[test]
fn open_fails_with_server_not_running_when_nothing_answers_on_the_port() {
    let home = Home::new();
    let board = board_dir(&home);
    let free = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = free.local_addr().unwrap().port();
    drop(free);
    write_server_file(&home, &board, port);
    home.run(&["open", "--board", board.to_str().unwrap()])
        .assert_failed("flare::server::not_running");
    assert!(home.opened().is_empty());
}

#[test]
fn open_names_an_untrusted_server_file_in_the_path_field() {
    let home = Home::new();
    let board = board_dir(&home);
    let free = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = free.local_addr().unwrap().port();
    drop(free);
    write_server_file(&home, &board, port);
    let file = StateDir::for_board(&home.state(), &board)
        .path()
        .join("server.json");
    let chmod = |mode| {
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    let args = ["open", "--board", board.to_str().unwrap(), "--json"];
    chmod(0o644);
    let ran = home.run(&args);
    ran.assert_failed("flare::state::unusable");
    let printed: serde_json::Value = serde_json::from_str(ran.stdout.trim()).unwrap();
    let name = file.to_str().unwrap();
    assert_eq!(
        printed["fields"],
        serde_json::json!({"path": name}),
        "{printed}"
    );
    assert_eq!(
        printed["message"],
        format!("{name} is not safe to use: it has mode 0644; remove it and run flare again"),
        "{printed}"
    );
    chmod(0o600);
    home.run(&args).assert_failed("flare::server::not_running");
}

#[test]
fn open_sends_no_token_to_a_server_that_gives_a_wrong_proof() {
    let home = Home::new();
    let board = board_dir(&home);
    let (port, heads) = fake_server("not-the-proof");
    write_server_file(&home, &board, port);
    home.run(&["open", "--board", board.to_str().unwrap()])
        .assert_failed("flare::server::not_running");
    let heads = heads.lock().unwrap();
    assert_eq!(heads.len(), 1, "only the probe was sent: {heads:?}");
    assert!(
        heads[0].starts_with("GET /launch/probe?nonce="),
        "{}",
        heads[0]
    );
    for head in heads.iter() {
        assert!(
            !head.to_ascii_lowercase().contains("authorization"),
            "{head}"
        );
    }
    assert!(home.opened().is_empty());
}

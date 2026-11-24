//! Plan test 16, the server's process: `flare serve` refuses an old board
//! program, a missing board, a directory that is not a board and a bad
//! config before it binds; it serves until Ctrl-C, removes `server.json`
//! and exits 130; `flare open` finds it; and no command prints the token
//! or the launch host.
#![cfg(test)]

#[path = "../../flare-board/tests/support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch board is flare-board's test support, and these tests use part of it"
)]
pub mod board_scratch;
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

use board_scratch::ScratchBoard;
use flare_auth_loopback::StateDir;
use run::{Home, Ran};
use secrecy::ExposeSecret as _;
use std::io::{BufRead as _, BufReader};
use std::process::Stdio;
use std::time::{Duration, Instant};

/// What a board older than crumbkit/board e4ee3333 prints for `log --at`.
const USAGE_REFUSAL: &str = r#"printf '%s\n' '{"schema":"board/failure/v1","code":"board::cli::usage","message":"unexpected argument '"'"'--at'"'"' found","fields":{"argument":"--at"}}'
exit 2
"#;

fn serve_args<'a>(board: &'a ScratchBoard, root: &'a str, program: &'a str) -> Vec<&'a str> {
    let _ = board;
    vec!["serve", "--board", root, "--board-program", program]
}

fn paths(board: &ScratchBoard) -> (String, String) {
    (
        board.root().to_str().unwrap().to_owned(),
        board.program().to_str().unwrap().to_owned(),
    )
}

#[test]
fn serve_refuses_a_board_program_that_has_no_log_at() {
    let home = Home::new();
    let board = ScratchBoard::new();
    let stub = board.script("old-board", USAGE_REFUSAL);
    let (root, _) = paths(&board);
    let ran = home.run(&serve_args(&board, &root, stub.to_str().unwrap()));
    ran.assert_failed("flare::board::too_old");
    assert!(ran.all().contains("e4ee3333"), "{}", ran.all());
}

#[test]
fn serve_refuses_a_missing_board_and_a_directory_that_is_not_a_board() {
    let home = Home::new();
    let board = ScratchBoard::new();
    let (_, program) = paths(&board);
    let missing = home.path().join("no-such-board");
    home.run(&[
        "serve",
        "--board",
        missing.to_str().unwrap(),
        "--board-program",
        &program,
    ])
    .assert_failed("flare::board::not_found");
    let plain = home.path().join("home");
    home.run(&[
        "serve",
        "--board",
        plain.to_str().unwrap(),
        "--board-program",
        &program,
    ])
    .assert_failed("flare::board::not_a_board");
}

#[test]
fn serve_refuses_a_config_with_an_unknown_key_or_a_client_secret() {
    let home = Home::new();
    let board = ScratchBoard::new();
    let (root, program) = paths(&board);
    home.write_config(r#"{"oidc":{"issuer":"https://accounts.google.com","client_id":"x","admit":{"domain":"example.com","reach":"everything"}},"colour":"blue"}"#);
    home.run(&serve_args(&board, &root, &program))
        .assert_failed("flare::config::invalid");
    home.write_config(r#"{"oidc":{"issuer":"https://accounts.google.com","client_id":"x","client_secret":"not-here","admit":{"domain":"example.com","reach":"everything"}}}"#);
    let ran = home.run(&serve_args(&board, &root, &program));
    ran.assert_failed("flare::config::invalid");
    assert!(ran.all().contains("flare-oidc-google"), "{}", ran.all());
    assert!(!ran.all().contains("not-here"), "{}", ran.all());
    home.write_config("{ not json");
    home.run(&serve_args(&board, &root, &program))
        .assert_failed("flare::config::invalid");
    let directory = home.path().join("home");
    let mut args = serve_args(&board, &root, &program);
    args.extend(["--config", directory.to_str().unwrap()]);
    home.run(&args).assert_failed("flare::config::unreadable");
}

/// A hosted config whose secret file is `secret` and whose issuer is
/// `issuer`.
fn hosted_config(secret: &str, issuer: &str) -> String {
    format!(
        r#"{{"hosted":{{"origin":"https://flare.example.com","listen":"127.0.0.1:0"}},"oidc":{{"issuer":"{issuer}","client_id":"x","client_secret_file":"{secret}","admit":{{"domain":"example.com","reach":"everything"}}}}}}"#
    )
}

#[test]
fn serve_refuses_a_hosted_config_without_its_keys_naming_each_and_admits_a_whole_one() {
    let home = Home::new();
    let board = ScratchBoard::new();
    let (root, program) = paths(&board);
    let secret = home.path().join("client_secret");
    run::write(&secret, "made-up-secret");
    let whole = hosted_config(secret.to_str().unwrap(), "https://127.0.0.1:1");
    for (broken, key) in [
        (
            whole.replace("https://flare.example.com", "http://flare.example.com"),
            "hosted.origin",
        ),
        (
            whole.replace("https://flare.example.com", "https://flare.example.com/x"),
            "hosted.origin",
        ),
        (
            whole.replace(
                &format!(r#""client_secret_file":"{}","#, secret.display()),
                "",
            ),
            "oidc.client_secret_file",
        ),
        (
            whole.replace(secret.to_str().unwrap(), "client_secret"),
            "oidc.client_secret_file",
        ),
        (
            r#"{"hosted":{"origin":"https://flare.example.com","listen":"127.0.0.1:0"}}"#
                .to_owned(),
            "oidc",
        ),
    ] {
        home.write_config(&broken);
        let ran = home.run(&serve_args(&board, &root, &program));
        ran.assert_failed("flare::config::invalid");
        assert!(ran.all().contains(key), "{key}: {}", ran.all());
    }
    // The whole config passes the config and secret checks and stops at the
    // provider, which nothing serves.
    home.write_config(&whole);
    home.run(&serve_args(&board, &root, &program))
        .assert_failed("flare::oidc::provider_unavailable");
}

#[test]
fn serve_refuses_a_hosted_secret_file_that_is_missing_or_writable_by_others_naming_the_path() {
    use std::os::unix::fs::PermissionsExt as _;
    let home = Home::new();
    let board = ScratchBoard::new();
    let (root, program) = paths(&board);
    let secret = home.path().join("client_secret");
    home.write_config(&hosted_config(
        secret.to_str().unwrap(),
        "https://127.0.0.1:1",
    ));
    let missing = home.run(&serve_args(&board, &root, &program));
    missing.assert_failed("flare::oidc::secret_unavailable");
    assert!(
        missing.all().contains(secret.to_str().unwrap()),
        "{}",
        missing.all()
    );
    run::write(&secret, "made-up-secret");
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o666)).unwrap();
    let writable = home.run(&serve_args(&board, &root, &program));
    writable.assert_failed("flare::oidc::secret_unavailable");
    assert!(writable.all().contains("writable"), "{}", writable.all());
    assert!(
        !writable.all().contains("made-up-secret"),
        "{}",
        writable.all()
    );
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
    home.run(&serve_args(&board, &root, &program))
        .assert_failed("flare::oidc::provider_unavailable");
}

/// A running `flare serve` and the home it runs in.
struct Running {
    #[expect(
        clippy::disallowed_types,
        reason = "the test holds the server it started, and stop() waits for it"
    )]
    child: std::process::Child,
    state: StateDir,
}

impl Running {
    fn start(home: &Home, board: &ScratchBoard, extra: &[&str]) -> Self {
        Self::start_when(home, board, extra, |state| {
            state.read_server().ok().flatten().is_some()
        })
    }

    /// Starts `flare serve` and waits until `ready` holds for its state
    /// directory.
    fn start_when(
        home: &Home,
        board: &ScratchBoard,
        extra: &[&str],
        ready: impl Fn(&StateDir) -> bool,
    ) -> Self {
        let (root, program) = paths(board);
        let mut args = serve_args(board, &root, &program);
        args.extend(extra);
        #[expect(
            clippy::disallowed_methods,
            reason = "the test starts the built binary and stops it with a signal"
        )]
        let child = home
            .command(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let state = StateDir::for_board(&home.state(), &board.root());
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ready(&state) {
            assert!(
                Instant::now() < deadline,
                "flare serve wrote no server.json"
            );
            std::thread::yield_now();
        }
        Self { child, state }
    }

    /// Sends the server `signal` and returns what it printed.
    fn stop(mut self, signal: &str) -> Ran {
        #[expect(
            clippy::disallowed_methods,
            reason = "the test stops the built binary with a signal"
        )]
        let status = std::process::Command::new("kill")
            .args([signal, &self.child.id().to_string()])
            .status()
            .unwrap();
        assert!(status.success());
        let mut stderr = String::new();
        let mut reader = BufReader::new(self.child.stderr.take().unwrap());
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap() > 0 {
            stderr.push_str(&line);
            line.clear();
        }
        let output = self.child.wait_with_output().unwrap();
        Ran {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr,
        }
    }
}

#[test]
fn serve_runs_until_ctrl_c_then_removes_server_json_and_exits_130() {
    let home = Home::new();
    let board = ScratchBoard::new();
    let running = Running::start(&home, &board, &[]);
    let record = running.state.read_server().unwrap().unwrap();
    let token = record.token().expose_secret().to_owned();
    let host = record.launch_host().name().to_owned();
    let opened = home.run(&["open", "--board", board.root().to_str().unwrap()]);
    opened.assert_ok();
    let openers = home.opened();
    assert_eq!(openers.len(), 1, "{}", opened.all());
    assert!(openers[0].starts_with(running.state.path()));
    let state = running.state.clone();
    let ran = running.stop("-INT");
    assert_eq!(ran.code, Some(130), "{}", ran.all());
    assert!(
        state.read_server().unwrap().is_none(),
        "server.json is removed"
    );
    assert!(ran.all().contains("flare open"), "{}", ran.all());
    for printed in [&ran, &opened] {
        assert!(!printed.all().contains(&token), "{}", printed.all());
        assert!(!printed.all().contains(&host), "{}", printed.all());
    }
}

/// Follows the launch URL in the opener file at `opener` and returns the
/// path that the launch document sends the browser on to.
fn launch_target(opener: &std::path::Path) -> String {
    let page = std::fs::read_to_string(opener).unwrap();
    let start = page.find("url=http://").unwrap() + "url=http://".len();
    let rest = &page[start..];
    let url = &rest[..rest.find('"').unwrap()];
    let (authority, path) = url.split_at(url.find('/').unwrap());
    let port = authority.rsplit(':').next().unwrap();
    let mut stream =
        std::net::TcpStream::connect(("127.0.0.1", port.parse::<u16>().unwrap())).unwrap();
    std::io::Write::write_all(
        &mut stream,
        format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n").as_bytes(),
    )
    .unwrap();
    let mut reply = String::new();
    std::io::Read::read_to_string(&mut stream, &mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    let key = "content=\"0;url=";
    let at = reply.find(key).unwrap() + key.len();
    reply[at..at + reply[at..].find('"').unwrap()].to_owned()
}

#[test]
fn open_names_a_page_or_a_binding_and_refuses_an_unknown_name() {
    let home = Home::new();
    let board = ScratchBoard::new();
    board.seed_record(
        "/tasks/tap",
        r#"{"$type":"tasks/task/v2","title":"Replace the tap washer"}"#,
    );
    let page = home.path().join("card.html");
    run::write(&page, "<p>{{task.title}}</p>");
    let slots = home.path().join("slots.json");
    run::write(
        &slots,
        r#"[{"name":"task","type":"tasks/task/v2","editable_fields":[],"sample":{"type":"tasks/task/v2","body":{"title":"A sample"}}}]"#,
    );
    let (root, program) = paths(&board);
    home.run(&[
        "publish",
        page.to_str().unwrap(),
        "--name",
        "task-card",
        "--slots",
        slots.to_str().unwrap(),
        "--board",
        &root,
        "--board-program",
        &program,
    ])
    .assert_ok();
    home.run(&[
        "bind",
        "task-card",
        "--name",
        "kitchen",
        "task=/tasks/tap",
        "--board",
        &root,
        "--board-program",
        &program,
    ])
    .assert_ok();
    let running = Running::start(&home, &board, &[]);
    for (name, target) in [
        (None, "/flare/"),
        (Some("task-card"), "/pages/task-card"),
        (Some("kitchen"), "/b/kitchen"),
    ] {
        let mut args = vec!["open", "--board", &root];
        args.extend(name);
        home.run(&args).assert_ok();
        let opener = home.opened().pop().unwrap();
        assert_eq!(launch_target(&opener), target);
    }
    home.run(&["open", "no-such-page", "--board", &root])
        .assert_failed("flare::pages::not_found");
    running.stop("-TERM");
}

#[test]
fn serve_stops_on_sigterm_and_exits_0() {
    let home = Home::new();
    let board = ScratchBoard::new();
    let running = Running::start(&home, &board, &["--json"]);
    let state = running.state.clone();
    let ran = running.stop("-TERM");
    assert_eq!(ran.code, Some(0), "{}", ran.all());
    assert!(ran.stdout.contains("\"flare/serve/v1\""), "{}", ran.stdout);
    assert!(state.read_server().unwrap().is_none());
}

#[test]
fn serve_exits_after_its_idle_period() {
    let home = Home::new();
    let board = ScratchBoard::new();
    let (root, program) = paths(&board);
    let mut args = serve_args(&board, &root, &program);
    args.extend(["--idle-exit", "1s"]);
    let ran = home.run(&args);
    ran.assert_ok();
    assert!(ran.all().contains("idle"), "{}", ran.all());
    let state = StateDir::for_board(&home.state(), &board.root());
    assert!(state.read_server().unwrap().is_none());
}

#[test]
fn a_second_serve_for_the_same_board_is_refused_while_the_first_answers() {
    let home = Home::new();
    let board = ScratchBoard::new();
    let running = Running::start(&home, &board, &[]);
    let (root, program) = paths(&board);
    let mut args = serve_args(&board, &root, &program);
    args.push("--json");
    let ran = home.run(&args);
    ran.assert_failed("flare::server::already_running");
    let printed: serde_json::Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        printed["recovery"],
        serde_json::json!({"kind": "run", "call": format!("flare open --board {root}")}),
        "{}",
        ran.all()
    );
    running.stop("-TERM");
}

/// Crash point 2: a server killed while it serves leaves `server.json`
/// stale. `flare open` then fails with `server::not_running`, and the next
/// `flare serve` replaces the file with its own.
#[test]
fn the_next_serve_replaces_the_server_json_a_killed_server_left() {
    let home = Home::new();
    let board = ScratchBoard::new();
    let root = board.root().to_str().unwrap().to_owned();
    let killed = Running::start(&home, &board, &[]);
    let state = killed.state.clone();
    let token = |state: &StateDir| {
        state
            .read_server()
            .unwrap()
            .map(|record| record.token().expose_secret().to_owned())
    };
    let stale = token(&state).unwrap();
    let ran = killed.stop("-KILL");
    assert_eq!(ran.code, None, "SIGKILL ends the server");
    assert_eq!(
        token(&state).as_ref(),
        Some(&stale),
        "the file is left stale"
    );
    home.run(&["open", "--board", &root])
        .assert_failed("flare::server::not_running");
    let next = Running::start_when(&home, &board, &[], |state| {
        token(state).is_some_and(|current| current != stale)
    });
    home.run(&["open", "--board", &root]).assert_ok();
    let ran = next.stop("-TERM");
    assert!(!ran.all().contains(&stale), "{}", ran.all());
}

#[test]
fn serve_refuses_a_port_that_another_program_holds() {
    let home = Home::new();
    let board = ScratchBoard::new();
    let held = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = held.local_addr().unwrap().port().to_string();
    let (root, program) = paths(&board);
    let mut args = serve_args(&board, &root, &program);
    args.extend(["--port", &port, "--json"]);
    let ran = home.run(&args);
    ran.assert_failed("flare::server::port_in_use");
    let printed: serde_json::Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        printed["recovery"],
        serde_json::json!({"kind": "run", "call": format!("flare serve --board {root}")}),
        "{}",
        ran.all()
    );
    drop(held);
}

//! `flare metrics` totals the Flare EMF lines of a log by day and metric,
//! skips every other line and says how many it skipped, and reads files or
//! standard input. `flare serve` with `log.metrics` writes those lines as
//! bare JSON on standard error, and without it writes none.
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
use serde_json::{Value, json};
use std::io::{Read as _, Write as _};
use std::process::Stdio;
use std::time::{Duration, Instant};

/// 2026-11-23T09:00:00Z and 2026-11-24T09:00:00Z, in milliseconds.
const MONDAY: u64 = 1_795_424_400_000;
const TUESDAY: u64 = MONDAY + 86_400_000;

/// A Flare EMF line of `metric` at `millis`, as flare-server writes it.
fn emf(metric: &str, millis: u64) -> String {
    json!({
        "_aws": {
            "Timestamp": millis,
            "CloudWatchMetrics": [{
                "Namespace": "Flare",
                "Dimensions": [[]],
                "Metrics": [{"Name": metric, "Unit": "Count"}],
            }],
        },
        metric: 1,
    })
    .to_string()
}

/// A log of five Flare lines over two days and four other lines.
fn log_text() -> String {
    [
        "2026-11-23T09:00:00.000000Z  INFO flare serves this board".to_owned(),
        emf("PagesViewed", MONDAY),
        emf("PagesViewed", MONDAY),
        "2026-11-23T09:00:00.100000Z  INFO served request=1 method=GET route=\"/up\" status=200 ms=0".to_owned(),
        emf("SignIns", MONDAY),
        emf("PagesViewed", TUESDAY),
        emf("Other", TUESDAY).replace("\"Flare\"", "\"Elsewhere\""),
        r#"{"_aws":{"Timestamp":1795424400000,"#.to_owned(),
        emf("SharesCreated", TUESDAY),
    ]
    .join("\n")
        + "\n"
}

/// The JSON document a run printed.
fn document(ran: &Ran) -> Value {
    ran.assert_ok();
    serde_json::from_str(ran.stdout.trim()).unwrap()
}

/// The `(metric, count)` totals of a document.
fn totals(document: &Value) -> Vec<(String, u64)> {
    document["summary"]["totals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|total| {
            (
                total["metric"].as_str().unwrap().to_owned(),
                total["count"].as_u64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn flare_metrics_totals_each_metric_by_day_and_reports_the_lines_it_skipped() {
    let home = Home::new();
    let file = home.path().join("flare.log");
    run::write(&file, &log_text());
    let printed = document(&home.run(&["metrics", file.to_str().unwrap(), "--json"]));
    assert_eq!(printed["schema"], "flare/metrics/v1", "{printed}");
    assert_eq!(
        printed["data"],
        json!([
            {"day": "2026-11-23", "metric": "PagesViewed", "count": 2},
            {"day": "2026-11-23", "metric": "SignIns", "count": 1},
            {"day": "2026-11-24", "metric": "PagesViewed", "count": 1},
            {"day": "2026-11-24", "metric": "SharesCreated", "count": 1},
        ]),
        "{printed}"
    );
    assert_eq!(
        totals(&printed),
        [
            ("PagesViewed".to_owned(), 3),
            ("SharesCreated".to_owned(), 1),
            ("SignIns".to_owned(), 1),
        ]
    );
    assert_eq!(printed["summary"]["lines"], 9);
    assert_eq!(printed["summary"]["skipped"], 4);

    let text = home.run(&["metrics", file.to_str().unwrap()]);
    text.assert_ok();
    assert!(text.stdout.contains("2026-11-24"), "{}", text.all());
    assert!(text.stdout.contains("PagesViewed"), "{}", text.all());
    assert!(text.all().contains("skipped 4"), "{}", text.all());
}

#[test]
fn flare_metrics_adds_up_every_file_it_is_given_and_reads_standard_input_without_one() {
    let home = Home::new();
    let older = home.path().join("flare.log.1");
    let newer = home.path().join("flare.log");
    run::write(&older, &log_text());
    run::write(&newer, &(emf("PagesViewed", TUESDAY) + "\n"));
    let both = document(&home.run(&[
        "metrics",
        older.to_str().unwrap(),
        newer.to_str().unwrap(),
        "--json",
    ]));
    assert_eq!(totals(&both)[0], ("PagesViewed".to_owned(), 4), "{both}");
    assert_eq!(both["summary"]["lines"], 10);

    let piped = document(&run_with_input(&home, &["metrics", "--json"], &log_text()));
    assert_eq!(totals(&piped)[0], ("PagesViewed".to_owned(), 3), "{piped}");
    assert_eq!(piped["summary"]["skipped"], 4);
}

#[test]
fn flare_metrics_refuses_a_file_it_cannot_read_and_reads_one_it_can() {
    let home = Home::new();
    let missing = home.path().join("no-such.log");
    let ran = home.run(&["metrics", missing.to_str().unwrap()]);
    ran.assert_failed("flare::metrics::unreadable");
    assert!(ran.all().contains("no-such.log"), "{}", ran.all());
    let json = home.run(&["metrics", missing.to_str().unwrap(), "--json"]);
    json.assert_failed("flare::metrics::unreadable");
    let printed: Value = serde_json::from_str(json.stdout.trim()).unwrap();
    assert_eq!(
        printed["recovery"],
        json!({"kind": "hint", "text": "pass a readable log file"}),
        "{}",
        json.all()
    );
    assert_eq!(
        printed["alternatives"],
        json!([{"kind": "hint", "text": "pipe the log to flare metrics on standard input"}]),
        "{}",
        json.all()
    );
    run::write(&missing, &log_text());
    home.run(&["metrics", missing.to_str().unwrap()])
        .assert_ok();
}

/// Runs the binary with `args` and `input` on its standard input.
fn run_with_input(home: &Home, args: &[&str], input: &str) -> Ran {
    #[expect(
        clippy::disallowed_methods,
        reason = "the test runs the built binary with input on a pipe"
    )]
    let mut child = home
        .command(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    Ran::from(child.wait_with_output().unwrap())
}

/// Starts `flare serve` on `board`, opens its index once, which makes a
/// few requests, stops it with SIGTERM and returns its standard error.
fn serve_once(home: &Home, board: &ScratchBoard) -> String {
    let root = board.root().to_str().unwrap().to_owned();
    let program = board.program().to_str().unwrap().to_owned();
    #[expect(
        clippy::disallowed_methods,
        reason = "the test starts the built binary and stops it with a signal"
    )]
    let mut child = home
        .command(&["serve", "--board", &root, "--board-program", &program])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let state = StateDir::for_board(&home.state(), &board.root());
    let deadline = Instant::now() + Duration::from_secs(30);
    while state.read_server().ok().flatten().is_none() {
        assert!(
            Instant::now() < deadline,
            "flare serve wrote no server.json"
        );
        std::thread::yield_now();
    }
    home.run(&["open", "--board", &root]).assert_ok();
    #[expect(
        clippy::disallowed_methods,
        reason = "the test stops the built binary with a signal"
    )]
    let status = std::process::Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(0), "{stderr}");
    stderr
}

#[test]
fn serve_with_log_metrics_writes_bare_emf_lines_that_flare_metrics_totals_and_without_it_writes_none()
 {
    let home = Home::new();
    let board = ScratchBoard::new();
    let off = serve_once(&home, &board);
    assert!(off.contains(" served "), "{off}");
    assert!(!off.contains("_aws"), "metrics are off by default: {off}");
    for line in off.lines().filter(|line| line.contains(" served ")) {
        assert!(line.contains("  INFO served request="), "{line}");
    }

    home.write_config(r#"{"log":{"metrics":true}}"#);
    let on = serve_once(&home, &board);
    let served = on.lines().filter(|line| line.contains(" served ")).count();
    assert!(served > 0, "{on}");
    let metric_lines: Vec<&str> = on.lines().filter(|line| line.contains("_aws")).collect();
    assert_eq!(metric_lines.len(), served, "{on}");
    for line in &metric_lines {
        assert!(line.starts_with('{') && line.ends_with('}'), "{line}");
        let _: Value = serde_json::from_str(line).unwrap();
    }
    let read = document(&run_with_input(&home, &["metrics", "--json"], &on));
    assert_eq!(
        totals(&read),
        [("Requests".to_owned(), u64::try_from(served).unwrap())],
        "{read}"
    );
    assert_eq!(
        read["summary"]["skipped"].as_u64().unwrap(),
        u64::try_from(on.lines().count() - served).unwrap()
    );
}

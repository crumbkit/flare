//! Plan test 16, the writes: `flare publish` refuses a template with
//! flaws, `flare bind` refuses a record that does not fit, names that try
//! to leave their segment are refused, and `--dry-run` writes nothing.
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
use run::{Home, write};
use std::path::PathBuf;

/// A task in the board that the binding tests fill a slot with.
const TASK: &str = r#"{"$type":"tasks/task/v2","title":"Replace the tap washer"}"#;

/// A slots file with one editable task slot.
const SLOTS: &str = r#"[{"name":"task","type":"tasks/task/v2","editable_fields":["title"],"sample":{"type":"tasks/task/v2","body":{"title":"A sample task"}}}]"#;

struct Setup {
    home: Home,
    board: ScratchBoard,
}

impl Setup {
    fn new() -> Self {
        let board = ScratchBoard::new();
        board.seed_record("/tasks/tap", TASK);
        Self {
            home: Home::new(),
            board,
        }
    }

    fn file(&self, name: &str, text: &str) -> PathBuf {
        let path = self.home.path().join(name);
        write(&path, text);
        path
    }

    fn flare(&self, args: &[&str]) -> run::Ran {
        let root = self.board.root();
        let program = self.board.program().to_path_buf();
        let mut all: Vec<&str> = args.to_vec();
        all.extend([
            "--board",
            root.to_str().unwrap(),
            "--board-program",
            program.to_str().unwrap(),
        ]);
        self.home.run(&all)
    }

    fn publish(&self, template: &str, slots: Option<&str>, extra: &[&str]) -> run::Ran {
        let page = self.file("card.html", template);
        let mut args = vec!["publish", page.to_str().unwrap(), "--name", "task-card"];
        let slots_file = slots.map(|text| self.file("slots.json", text));
        if let Some(path) = &slots_file {
            args.extend(["--slots", path.to_str().unwrap()]);
        }
        args.extend(extra);
        self.flare(&args)
    }
}

#[test]
fn publish_refuses_a_template_with_flaws_and_writes_nothing() {
    let setup = Setup::new();
    let before = setup.board.commits();
    setup
        .publish("<p>{{#open}}</p>", None, &[])
        .assert_failed("flare::pages::template_flaws");
    assert_eq!(setup.board.commits(), before);
}

#[test]
fn publish_stores_a_page_whose_template_has_no_flaws() {
    let setup = Setup::new();
    let before = setup.board.commits();
    let ran = setup.publish("<h1>{{task.title}}</h1>", Some(SLOTS), &["--json"]);
    ran.assert_ok();
    assert!(
        ran.stdout.contains("\"flare/publish/v1\""),
        "{}",
        ran.stdout
    );
    assert!(setup.board.commits() > before);
    assert!(
        setup
            .board
            .file("pages/task-card/page.json")
            .contains("tasks/task/v2")
    );
}

#[test]
fn publish_with_dry_run_checks_the_page_and_writes_nothing() {
    let setup = Setup::new();
    let before = setup.board.commits();
    let ran = setup.publish("<h1>{{task.title}}</h1>", Some(SLOTS), &["--dry-run"]);
    ran.assert_ok();
    assert!(ran.stderr.contains("dry run"), "{}", ran.all());
    assert_eq!(setup.board.commits(), before);
    setup
        .publish("<p>{{#open}}</p>", None, &["--dry-run"])
        .assert_failed("flare::pages::template_flaws");
}

#[test]
fn publish_refuses_a_file_that_is_neither_html_nor_markdown() {
    let setup = Setup::new();
    let page = setup.file("card.txt", "<p>x</p>");
    setup
        .flare(&["publish", page.to_str().unwrap(), "--name", "task-card"])
        .assert_failed("flare::pages::unknown_format");
    let markdown = setup.file("card.md", "# A card");
    setup
        .flare(&["publish", markdown.to_str().unwrap(), "--name", "task-card"])
        .assert_ok();
}

#[test]
fn publish_and_bind_refuse_names_that_are_not_one_segment() {
    let setup = Setup::new();
    let page = setup.file("card.html", "<p>x</p>");
    for name in ["..", "a/b", "../kitchen", "Kitchen", ""] {
        let ran = setup.flare(&["publish", page.to_str().unwrap(), "--name", name]);
        ran.assert_failed("flare::pages::invalid");
        let ran = setup.flare(&["bind", "task-card", "--name", name, "task=/tasks/tap"]);
        ran.assert_failed("flare::bindings::invalid");
    }
    for fill in ["task", "task=", "task=../tasks/tap", "=/tasks/tap"] {
        let ran = setup.flare(&["bind", "task-card", "--name", "kitchen", fill]);
        ran.assert_failed("flare::bindings::invalid");
    }
}

#[test]
fn bind_refuses_a_slot_that_the_page_does_not_declare() {
    let setup = Setup::new();
    setup.publish("<p>plain</p>", None, &[]).assert_ok();
    let before = setup.board.commits();
    setup
        .flare(&["bind", "task-card", "--name", "kitchen", "task=/tasks/tap"])
        .assert_failed("flare::bindings::misfit");
    assert_eq!(setup.board.commits(), before);
}

#[test]
fn bind_saves_a_binding_whose_records_fit_the_page() {
    let setup = Setup::new();
    setup
        .publish("<h1>{{task.title}}</h1>", Some(SLOTS), &[])
        .assert_ok();
    let ran = setup.flare(&[
        "bind",
        "task-card",
        "--name",
        "kitchen",
        "task=/tasks/tap",
        "--json",
    ]);
    ran.assert_ok();
    assert!(ran.stdout.contains("\"flare/bind/v1\""), "{}", ran.stdout);
    assert!(
        setup
            .board
            .file("bindings/kitchen.json")
            .contains("/tasks/tap")
    );
    setup
        .flare(&["bind", "task-card", "--name", "kitchen", "task=/tasks/tap"])
        .assert_failed("flare::bindings::name_taken");
}

#[test]
fn bind_refuses_a_record_that_does_not_exist_and_an_unknown_page() {
    let setup = Setup::new();
    setup
        .publish("<h1>{{task.title}}</h1>", Some(SLOTS), &[])
        .assert_ok();
    setup
        .flare(&["bind", "task-card", "--name", "kitchen", "task=/tasks/none"])
        .assert_failed("flare::bindings::record_not_found");
    setup
        .flare(&["bind", "no-page", "--name", "kitchen", "task=/tasks/tap"])
        .assert_failed("flare::pages::not_found");
}

#[test]
fn bind_with_dry_run_checks_the_fit_and_writes_nothing() {
    let setup = Setup::new();
    setup
        .publish("<h1>{{task.title}}</h1>", Some(SLOTS), &[])
        .assert_ok();
    let before = setup.board.commits();
    let ran = setup.flare(&[
        "bind",
        "task-card",
        "--name",
        "kitchen",
        "task=/tasks/tap",
        "--dry-run",
    ]);
    ran.assert_ok();
    assert_eq!(setup.board.commits(), before);
    setup
        .flare(&[
            "bind",
            "task-card",
            "--name",
            "kitchen",
            "other=/tasks/tap",
            "--dry-run",
        ])
        .assert_failed("flare::bindings::misfit");
    setup
        .flare(&[
            "bind",
            "task-card",
            "--name",
            "kitchen",
            "task=/tasks/none",
            "--dry-run",
        ])
        .assert_failed("flare::bindings::record_not_found");
    assert_eq!(setup.board.commits(), before);
}

#[test]
fn a_run_appends_one_line_to_the_usage_log() {
    let setup = Setup::new();
    assert_eq!(setup.home.usage_lines(), 0);
    setup.publish("<p>x</p>", None, &["--dry-run"]).assert_ok();
    assert_eq!(setup.home.usage_lines(), 1);
    setup.home.run(&["self", "usage"]).assert_ok();
    assert_eq!(setup.home.usage_lines(), 2);
}

/// The failure document that a `--json` run printed, after asserting that
/// the run failed with `code`.
fn failure(ran: &run::Ran, code: &str) -> serde_json::Value {
    ran.assert_failed(code);
    let printed: serde_json::Value = serde_json::from_str(ran.stdout.trim()).unwrap();
    assert_eq!(printed["code"], code, "{}", ran.all());
    printed
}

#[test]
fn publish_names_a_board_program_it_cannot_find_in_the_program_field() {
    let setup = Setup::new();
    let page = setup.file("card.html", "<p>x</p>");
    let root = setup.board.root();
    let args = [
        "publish",
        page.to_str().unwrap(),
        "--name",
        "task-card",
        "--json",
    ];
    let mut missing = args.to_vec();
    missing.extend([
        "--board",
        root.to_str().unwrap(),
        "--board-program",
        "/missing/board",
    ]);
    let printed = failure(
        &setup.home.run(&missing),
        "flare::board::program_unavailable",
    );
    assert_eq!(
        printed["fields"],
        serde_json::json!({"program": "/missing/board"}),
        "{printed}"
    );
    setup.flare(&args).assert_ok();
}

#[test]
fn publish_names_a_state_directory_it_cannot_create_in_the_path_field() {
    let setup = Setup::new();
    let page = setup.file("card.html", "<p>x</p>");
    let blocked = setup.home.path().join("blocked-state");
    std::fs::create_dir(&blocked).unwrap();
    write(
        &blocked.join("flare"),
        "a file where flare wants a directory",
    );
    let args = [
        "publish",
        page.to_str().unwrap(),
        "--name",
        "task-card",
        "--json",
    ];
    let root = setup.board.root();
    let program = setup.board.program().to_path_buf();
    #[expect(clippy::disallowed_methods, reason = "the test runs the built binary")]
    let output = setup
        .home
        .command(&args)
        .args([
            "--board",
            root.to_str().unwrap(),
            "--board-program",
            program.to_str().unwrap(),
        ])
        .env("XDG_STATE_HOME", &blocked)
        .output()
        .unwrap();
    let printed = failure(&run::Ran::from(output), "flare::state::unusable");
    let key = flare_auth_loopback::StateDir::for_board(&blocked, &root)
        .path()
        .file_name()
        .unwrap()
        .to_owned();
    let children = blocked.join("flare").join("board").join(key);
    assert_eq!(
        printed["fields"],
        serde_json::json!({"path": children.to_str().unwrap()}),
        "{printed}"
    );
    setup.flare(&args).assert_ok();
}

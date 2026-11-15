//! How `BoardStore` keeps record edits: the edit record beside each write,
//! Markdown records as JSON objects, the paths it refuses and the answers
//! for stale and refused bodies.
#![cfg(test)]

#[path = "support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch board is test support: it panics on a broken fixture, and each test file uses part of it"
)]
pub mod scratch;

use hangar_board::BoardStore;
use hangar_core::contract::block_on;
use hangar_core::{
    BindingName, Blob, Principal, PrincipalId, ReadError, RecordBody, RecordPath, RecordStore,
    WriteError, WriteProvenance,
};
use scratch::ScratchBoard;
use serde_json::{Value, json};

fn path(text: &str) -> RecordPath {
    RecordPath::try_from(text).unwrap()
}

fn body(value: &Value) -> RecordBody {
    RecordBody::try_from(value.to_string().as_str()).unwrap()
}

fn edit(based_on: &Blob) -> WriteProvenance {
    WriteProvenance::new(
        based_on.clone(),
        Principal::new(PrincipalId::try_from("ada").unwrap()),
        BindingName::try_from("my-view").unwrap(),
    )
}

fn object(store: &BoardStore, at: &str) -> Value {
    let record = block_on(store.read(&path(at))).unwrap();
    serde_json::from_str(record.body().as_str()).unwrap()
}

/// The records under /hangar/edits, as JSON values.
fn edits(board: &ScratchBoard) -> Vec<(String, Value)> {
    let listed = board.board(&["ls", "--json", "--all", "--", "/hangar/edits"], "");
    if !listed.status.success() {
        return Vec::new();
    }
    let report: Value = serde_json::from_slice(&listed.stdout).unwrap();
    let mut found = Vec::new();
    for row in report["data"].as_array().unwrap() {
        let logical = row["logical"].as_str().unwrap().to_owned();
        let got = board.board(&["get", "--json", "--", &logical], "");
        let got: Value = serde_json::from_slice(&got.stdout).unwrap();
        found.push((logical, got["data"][0]["record"].clone()));
    }
    found
}

fn last_commit_of(board: &ScratchBoard, logical: &str) -> String {
    let log = board.board(&["log", "--json", "--limit", "1", "--", logical], "");
    let log: Value = serde_json::from_slice(&log.stdout).unwrap();
    log["data"][0]["commit"].as_str().unwrap().to_owned()
}

fn seeded_task() -> ScratchBoard {
    let board = ScratchBoard::new();
    board.seed_record("/tasks/a", r#"{"$type":"tasks/task/v2","title":"first"}"#);
    board
}

#[test]
fn a_write_stores_one_edit_record_in_the_same_commit_as_the_edit() {
    let board = seeded_task();
    let store = board.store();
    let before = block_on(store.read(&path("/tasks/a"))).unwrap();
    let written = block_on(store.write(
        &path("/tasks/a"),
        &body(&json!({"$type": "tasks/task/v2", "title": "second"})),
        &edit(before.blob()),
    ))
    .unwrap();

    let found = edits(&board);
    assert_eq!(found.len(), 1, "{found:?}");
    let (logical, record) = found.first().unwrap();
    assert_eq!(
        record,
        &json!({
            "$type": "hangar/edit/v1",
            "record": "/tasks/a",
            "based_on": before.blob().as_str(),
            "made_by": "ada",
            "through": "my-view",
        })
    );
    assert_eq!(last_commit_of(&board, logical), written.as_str());
    assert_eq!(last_commit_of(&board, "/tasks/a"), written.as_str());
}

#[test]
fn a_refused_write_stores_no_edit_record() {
    let board = seeded_task();
    let store = board.store();
    let before = block_on(store.read(&path("/tasks/a"))).unwrap();
    let stale = Blob::try_from("f".repeat(40).as_str()).unwrap();
    let conflict = block_on(store.write(
        &path("/tasks/a"),
        &body(&json!({"$type": "tasks/task/v2", "title": "second"})),
        &edit(&stale),
    ));
    let rejected = block_on(store.write(
        &path("/tasks/a"),
        &body(&json!({"$type": "tasks/task/v2", "title": 5})),
        &edit(before.blob()),
    ));
    assert_eq!(conflict, Err(WriteError::Conflict));
    assert!(
        matches!(rejected, Err(WriteError::Rejected(_))),
        "{rejected:?}"
    );
    assert!(edits(&board).is_empty());
    assert_eq!(block_on(store.read(&path("/tasks/a"))).unwrap(), before);
}

const MARKDOWN_TASK: &str =
    "---\n$type: tasks/task/v2\ntitle: \"Plan: the week\"\nstatus: open\n---\n\nSome prose.\n";

#[test]
fn a_markdown_task_reads_as_one_json_object_with_its_prose_under_body() {
    let board = ScratchBoard::new();
    board.seed_text("/tasks/m.md", MARKDOWN_TASK);
    let store = board.store();
    let record = block_on(store.read(&path("/tasks/m.md"))).unwrap();
    assert_eq!(record.record_type().as_str(), "tasks/task/v2");
    let value: Value = serde_json::from_str(record.body().as_str()).unwrap();
    assert_eq!(
        value,
        json!({
            "$type": "tasks/task/v2",
            "title": "Plan: the week",
            "status": "open",
            "$body": "\nSome prose.\n",
        })
    );

    let commits = board.commits();
    let unchanged =
        block_on(store.write(&path("/tasks/m.md"), record.body(), &edit(record.blob()))).unwrap();
    assert_eq!(&unchanged, record.blob());
    assert_eq!(board.commits(), commits);
    assert_eq!(board.file("tasks/m.md"), MARKDOWN_TASK);
}

#[test]
fn front_matter_the_schema_refuses_is_rejected_and_changes_nothing() {
    let board = ScratchBoard::new();
    board.seed_text("/tasks/m.md", MARKDOWN_TASK);
    let store = board.store();
    let record = block_on(store.read(&path("/tasks/m.md"))).unwrap();
    let answer = block_on(store.write(
        &path("/tasks/m.md"),
        &body(&json!({"$type": "tasks/task/v2", "title": 5, "$body": "x\n"})),
        &edit(record.blob()),
    ));
    assert!(matches!(answer, Err(WriteError::Rejected(_))), "{answer:?}");
    assert_eq!(board.file("tasks/m.md"), MARKDOWN_TASK);
    assert!(edits(&board).is_empty());
}

#[test]
fn a_markdown_write_keeps_the_field_order_and_the_prose_bytes() {
    let board = ScratchBoard::new();
    let prose = "line\n---\nafter\n";
    board.seed_text(
        "/tasks/m.md",
        &format!(
            "---\n$type: tasks/task/v2\ntitle: \"T\"\nstatus: open\nparent: /tasks/p\n---\n{prose}"
        ),
    );
    let store = board.store();
    let record = block_on(store.read(&path("/tasks/m.md"))).unwrap();
    let mut value: Value = serde_json::from_str(record.body().as_str()).unwrap();
    assert_eq!(value["$body"], prose);
    let fields = value.as_object_mut().unwrap();
    fields.remove("parent");
    fields.insert("status".into(), json!("done"));
    fields.insert("due".into(), json!("2026-11-20"));
    let sorted = serde_json::to_string_pretty(&value).unwrap();

    block_on(store.write(
        &path("/tasks/m.md"),
        &RecordBody::try_from(sorted.as_str()).unwrap(),
        &edit(record.blob()),
    ))
    .unwrap();
    assert_eq!(
        board.file("tasks/m.md"),
        format!(
            "---\n$type: \"tasks/task/v2\"\ntitle: \"T\"\nstatus: \"done\"\ndue: \"2026-11-20\"\n---\n{prose}"
        )
    );
}

#[test]
fn every_task_field_round_trips_as_a_json_valued_line() {
    let board = ScratchBoard::new();
    board.seed_text("/tasks/m.md", MARKDOWN_TASK);
    let store = board.store();
    let fields = json!({
        "$type": "tasks/task/v2",
        "title": "a \"quoted\": #tag\\n and\nnext",
        "after": ["/tasks/b", "/tasks/c"],
        "contexts": ["errands"],
        "ephemeral": true,
        "repeat": "every day",
        "from_hour": 9,
        "owner": {"person": "ana", "program": "claude-code"},
        "$body": "prose\n",
    });
    for pinned in [json!(true), json!(3)] {
        let record = block_on(store.read(&path("/tasks/m.md"))).unwrap();
        let mut wanted = fields.clone();
        wanted["pinned"] = pinned;
        block_on(store.write(&path("/tasks/m.md"), &body(&wanted), &edit(record.blob()))).unwrap();
        assert_eq!(object(&store, "/tasks/m.md"), wanted);
    }
}

#[test]
fn a_write_to_a_binding_through_the_record_store_is_rejected() {
    let board = ScratchBoard::new();
    board.seed_record("/bindings/x", r#"{"$type":"notes/note/v1","text":"held"}"#);
    let store = board.store();
    let record = block_on(store.read(&path("/bindings/x"))).unwrap();
    let answer = block_on(store.write(
        &path("/bindings/x"),
        &body(&json!({"$type": "notes/note/v1", "text": "changed"})),
        &edit(record.blob()),
    ));
    assert!(matches!(answer, Err(WriteError::Rejected(_))), "{answer:?}");
    assert_eq!(block_on(store.read(&path("/bindings/x"))).unwrap(), record);
}

#[test]
fn a_write_to_a_task_through_the_record_store_succeeds() {
    let board = seeded_task();
    let store = board.store();
    let record = block_on(store.read(&path("/tasks/a"))).unwrap();
    let wanted = json!({"$type": "tasks/task/v2", "title": "changed"});
    block_on(store.write(&path("/tasks/a"), &body(&wanted), &edit(record.blob()))).unwrap();
    assert_eq!(object(&store, "/tasks/a"), wanted);
}

#[test]
fn a_stale_blob_with_an_invalid_body_is_a_conflict() {
    let board = seeded_task();
    let store = board.store();
    let before = block_on(store.read(&path("/tasks/a"))).unwrap();
    let stale = Blob::try_from("e".repeat(40).as_str()).unwrap();
    let answer = block_on(store.write(
        &path("/tasks/a"),
        &body(&json!({"$type": "tasks/task/v2", "title": 5})),
        &edit(&stale),
    ));
    assert_eq!(answer, Err(WriteError::Conflict));
    assert_eq!(block_on(store.read(&path("/tasks/a"))).unwrap(), before);
}

#[test]
fn a_current_blob_with_an_invalid_body_is_rejected() {
    let board = seeded_task();
    let store = board.store();
    let before = block_on(store.read(&path("/tasks/a"))).unwrap();
    let answer = block_on(store.write(
        &path("/tasks/a"),
        &body(&json!({"$type": "tasks/task/v2", "title": 5})),
        &edit(before.blob()),
    ));
    assert!(matches!(answer, Err(WriteError::Rejected(_))), "{answer:?}");
    assert_eq!(block_on(store.read(&path("/tasks/a"))).unwrap(), before);
}

#[test]
fn a_write_to_a_record_that_is_a_symlink_is_rejected_and_changes_nothing() {
    let board = seeded_task();
    std::os::unix::fs::symlink("a.json", board.root().join("tasks/l.json")).unwrap();
    board.git(&["add", "tasks/l.json"]);
    board.git(&["commit", "-q", "-m", "link"]);
    let store = board.store();
    let through_link = block_on(store.read(&path("/tasks/l"))).unwrap();
    let answer = block_on(store.write(
        &path("/tasks/l"),
        &body(&json!({"$type": "tasks/task/v2", "title": "via the link"})),
        &edit(through_link.blob()),
    ));
    assert!(matches!(answer, Err(WriteError::Rejected(_))), "{answer:?}");
    assert_eq!(object(&store, "/tasks/a")["title"], "first");
    assert!(edits(&board).is_empty());
}

#[test]
fn a_record_reached_through_a_link_or_mount_is_unavailable() {
    let board = seeded_task();
    board.seed_record("/linked", r#"{"$type":"board/mount/v1","path":"/tasks"}"#);
    let store = board.store();
    assert_eq!(
        block_on(store.read(&path("/linked/a"))),
        Err(ReadError::Unavailable)
    );
    assert_eq!(
        block_on(store.read_all(&[path("/linked/a"), path("/tasks/a")]))
            .into_iter()
            .map(|answer| answer.map(|record| record.path().clone()))
            .collect::<Vec<_>>(),
        [Err(ReadError::Unavailable), Ok(path("/tasks/a"))]
    );
}

/// A board program that, before each put, commits a valid change to
/// /tasks/a with the real board, and then runs the put it was given.
fn racing_before_each_put(board: &ScratchBoard) -> std::path::PathBuf {
    let real = board.program().display().to_string();
    board.script(
        "racing-board",
        &format!(
            r#"case "$1" in
  put)
    printf '%s\n' '{{"path":"/tasks/a","record":{{"$type":"tasks/task/v2","title":"raced"}}}}' | "{real}" put --batch --json >/dev/null;;
esac
exec "{real}" "$@"
"#
        ),
    )
}

#[test]
fn an_invalid_body_whose_record_changed_before_the_put_is_a_conflict() {
    let board = seeded_task();
    let store = board.store();
    let before = block_on(store.read(&path("/tasks/a"))).unwrap();
    let racing = board.store_with(
        &racing_before_each_put(&board),
        hangar_board::Limits::default(),
    );
    let answer = block_on(racing.write(
        &path("/tasks/a"),
        &body(&json!({"$type": "tasks/task/v2", "title": 5})),
        &edit(before.blob()),
    ));
    assert_eq!(answer, Err(WriteError::Conflict));
    assert_eq!(object(&store, "/tasks/a")["title"], "raced");
    assert!(edits(&board).is_empty());
}

#[test]
fn read_all_answers_unavailable_when_its_files_come_from_two_commits() {
    let board = seeded_task();
    let entry = |file: &str, commit: &str| {
        json!({
            "location": {"path": file, "pointers": [], "source": commit},
            "blob": "b".repeat(40),
            "text": "{\"$type\":\"tasks/task/v2\"}",
            "record": {"$type": "tasks/task/v2"},
        })
    };
    let report = json!({
        "schema": "board/get-report/v2",
        "data": [
            entry("tasks/a.json", &"1".repeat(40)),
            entry("tasks/b.json", &"2".repeat(40)),
        ],
        "failures": [],
    });
    // The log answer would make each path readable on its own, so only the
    // check that every file comes from one commit makes them unavailable.
    let log = json!({
        "schema": "board/log-report/v2",
        "data": [{"commit": "1".repeat(40), "blob": "b".repeat(40)}],
        "summary": {"complete": true},
    });
    let two_sources = board.script(
        "two-sources-board",
        &format!(
            "case \"$1\" in\n  log) printf '%s\\n' '{log}';;\n  *) printf '%s\\n' '{report}';;\nesac\n"
        ),
    );
    let store = board.store_with(&two_sources, hangar_board::Limits::default());
    assert_eq!(
        block_on(store.read_all(&[path("/tasks/a"), path("/tasks/b")])),
        [Err(ReadError::Unavailable), Err(ReadError::Unavailable)]
    );
}

//! How `BoardStore` keeps page versions, and how it reads a binding's fills
//! at one commit.
#![cfg(test)]

#[path = "support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch board is test support: it panics on a broken fixture, and each test file uses part of it"
)]
pub mod scratch;

use flare_board::{BoardStore, Limits};
use flare_core::contract::block_on;
use flare_core::{
    ContentFormat, MadeFrom, Page, PageContent, PageName, PagePin, PageStore, PageStoreError,
    PageVersion, Principal, PrincipalId, Provenance, RecordPath, RecordStore, RecordType, Slot,
    SlotName, Slots,
};
use scratch::ScratchBoard;

fn page(format: ContentFormat, text: &str) -> Page {
    let slot = Slot::new(
        SlotName::try_from("tasks").unwrap(),
        RecordType::try_from("tasks/task/v2").unwrap(),
        Vec::new(),
        None,
    )
    .unwrap();
    Page::new(
        PageName::try_from("home").unwrap(),
        PageContent::new(format, text).unwrap(),
        Slots::new(vec![slot]).unwrap(),
    )
}

fn original() -> Provenance {
    Provenance::new(
        Principal::new(PrincipalId::try_from("ada").unwrap()),
        MadeFrom::Original,
        Vec::new(),
    )
    .unwrap()
}

fn publish(store: &BoardStore, format: ContentFormat, text: &str) -> PageVersion {
    block_on(store.publish(page(format, text), original())).unwrap()
}

fn home() -> PageName {
    PageName::try_from("home").unwrap()
}

fn path(text: &str) -> RecordPath {
    RecordPath::try_from(text).unwrap()
}

#[test]
fn a_republish_with_the_same_content_still_makes_a_new_version() {
    let board = ScratchBoard::new();
    let store = board.store();
    let first = publish(&store, ContentFormat::Html, "<p>hi</p>");
    let second = publish(&store, ContentFormat::Html, "<p>hi</p>");
    assert_ne!(first, second);
    assert_eq!(block_on(store.versions(&home())).unwrap(), [second, first]);
}

#[test]
fn a_format_change_removes_the_old_page_file() {
    let board = ScratchBoard::new();
    let store = board.store();
    publish(&store, ContentFormat::Html, "<p>hi</p>");
    assert!(board.root().join("pages/home/page.html").is_file());
    publish(&store, ContentFormat::Markdown, "# Hi\n");
    assert!(!board.root().join("pages/home/page.html").exists());
    assert_eq!(board.file("pages/home/page.md"), "# Hi\n");
    let latest = block_on(store.latest(&home())).unwrap();
    assert_eq!(latest.page().content().format(), ContentFormat::Markdown);
}

#[test]
fn at_refuses_a_commit_that_never_changed_the_page() {
    let board = ScratchBoard::new();
    let store = board.store();
    publish(&store, ContentFormat::Html, "one");
    board.seed_record("/tasks/a", r#"{"$type":"tasks/task/v2","title":"first"}"#);
    let unrelated = board.git(&["rev-parse", "HEAD"]);
    let pin = PagePin::new(home(), PageVersion::try_from(unrelated.trim()).unwrap());
    assert_eq!(block_on(store.at(&pin)), Err(PageStoreError::NotFound));
}

#[test]
fn at_returns_a_commit_that_changed_the_page() {
    let board = ScratchBoard::new();
    let store = board.store();
    let first = publish(&store, ContentFormat::Html, "one");
    publish(&store, ContentFormat::Html, "two");
    let older = block_on(store.at(&PagePin::new(home(), first.clone()))).unwrap();
    assert_eq!(older.page().content().text(), "one");
    assert_eq!(older.version(), &first);
}

#[test]
fn a_board_that_stops_committing_its_puts_after_open_cannot_publish() {
    let board = ScratchBoard::new();
    let store = board.store();
    #[expect(
        clippy::disallowed_methods,
        reason = "the test writes its board's configuration"
    )]
    std::fs::write(
        board.root().join(".board/board.json"),
        r#"{"$type":"board/config/v1","commit_on_put":false}"#,
    )
    .unwrap();
    let answer = block_on(store.publish(page(ContentFormat::Html, "one"), original()));
    assert_eq!(answer, Err(PageStoreError::Unavailable));
}

/// A board program that answers `get --at HEAD` as board does and then,
/// before it prints the answer, commits a change that turns /tasks/a into a
/// note.
fn committing_after_the_read(board: &ScratchBoard) -> std::path::PathBuf {
    let real = board.program().display().to_string();
    board.script(
        "committing-board",
        &format!(
            r#"case "$*" in
  *"--at HEAD"*)
    answer=$("{real}" "$@")
    status=$?
    printf '%s\n' '{{"path":"/tasks/a","record":{{"$type":"notes/note/v1","text":"now a note"}}}}' | "{real}" put --batch --json >/dev/null
    printf '%s\n' "$answer"
    exit $status;;
esac
exec "{real}" "$@"
"#
        ),
    )
}

#[test]
fn read_all_answers_the_records_at_one_commit_while_another_commit_lands() {
    let board = ScratchBoard::new();
    board.seed_record("/tasks/a", r#"{"$type":"tasks/task/v2","title":"first"}"#);
    board.seed_record("/notes/n", r#"{"$type":"notes/note/v1","text":"a note"}"#);
    let store = board.store();
    let before = [
        block_on(store.read(&path("/tasks/a"))).unwrap(),
        block_on(store.read(&path("/notes/n"))).unwrap(),
    ];

    let shim = committing_after_the_read(&board);
    let racing = board.store_with(&shim, Limits::default());
    let together = block_on(racing.read_all(&[path("/tasks/a"), path("/notes/n")]));
    let after = block_on(store.read(&path("/tasks/a"))).unwrap();

    assert_eq!(together, before.iter().cloned().map(Ok).collect::<Vec<_>>());
    assert_eq!(after.record_type().as_str(), "notes/note/v1");
    assert_ne!(after.blob(), before.first().unwrap().blob());
}

#[test]
fn read_all_answers_as_read_does_when_no_commit_lands() {
    let board = ScratchBoard::new();
    board.seed_record("/tasks/a", r#"{"$type":"tasks/task/v2","title":"first"}"#);
    board.seed_record("/notes/n", r#"{"$type":"notes/note/v1","text":"a note"}"#);
    let store = board.store();
    let paths = [path("/tasks/a"), path("/tasks/missing"), path("/notes/n")];
    let together = block_on(store.read_all(&paths));
    let alone: Vec<_> = paths.iter().map(|at| block_on(store.read(at))).collect();
    assert_eq!(together, alone);
}

#[test]
fn versions_answers_unavailable_when_the_log_stops_early() {
    let board = ScratchBoard::new();
    let store = board.store();
    publish(&store, ContentFormat::Html, "one");
    let real = board.program().display().to_string();
    let stopping = board.script(
        "stopping-board",
        &format!(
            r#"case "$*" in
  *"log --json --all"*)
    printf '%s\n' '{{"schema":"board/log-report/v2","data":[],"truncated":true,"summary":{{"complete":false}}}}'
    exit 0;;
esac
exec "{real}" "$@"
"#
        ),
    );
    let stopped = board.store_with(&stopping, Limits::default());
    assert_eq!(
        block_on(stopped.versions(&home())),
        Err(PageStoreError::Unavailable)
    );
}

#[test]
fn bindings_of_a_page_that_does_not_exist_is_not_found() {
    let board = ScratchBoard::new();
    let store = board.store();
    assert_eq!(
        block_on(store.bindings_of(&home())),
        Err(PageStoreError::NotFound)
    );
}

/// The version that the page record in the working tree names as the one
/// it replaced.
fn previous_in_tree(board: &ScratchBoard) -> serde_json::Value {
    let record: serde_json::Value =
        serde_json::from_str(&board.file("pages/home/page.json")).unwrap();
    record.get("previous").cloned().unwrap_or_default()
}

#[test]
fn a_page_whose_first_publish_was_left_uncommitted_publishes_again_with_no_previous() {
    let board = ScratchBoard::new();
    let store = board.store();
    publish(&store, ContentFormat::Html, "one");
    // The reviewer's probe: undo the first publish's commit and keep its
    // files, as a put stopped after board wrote them leaves the tree.
    board.git(&["reset", "-q", "HEAD~1"]);
    assert_eq!(
        block_on(store.latest(&home())).map(|_| ()),
        Err(PageStoreError::NotFound)
    );

    let recovered = publish(&store, ContentFormat::Html, "two");

    assert_eq!(block_on(store.versions(&home())).unwrap(), [recovered]);
    assert_eq!(previous_in_tree(&board), serde_json::Value::Null);
    let latest = block_on(store.latest(&home())).unwrap();
    assert_eq!(latest.page().content().text(), "two");
}

#[test]
fn a_second_publish_after_a_committed_first_names_the_first_as_previous() {
    let board = ScratchBoard::new();
    let store = board.store();
    let first = publish(&store, ContentFormat::Html, "one");
    let second = publish(&store, ContentFormat::Html, "two");
    assert_eq!(
        previous_in_tree(&board),
        serde_json::Value::String(first.as_str().to_owned())
    );
    assert_eq!(block_on(store.versions(&home())).unwrap(), [second, first]);
}

#[test]
fn a_record_that_a_complete_walk_finds_no_commit_for_publishes_with_no_previous() {
    let board = ScratchBoard::new();
    let store = board.store();
    publish(&store, ContentFormat::Html, "one");
    board.git(&["reset", "-q", "HEAD~1"]);
    // A board program whose walk of the record's newest row from HEAD is
    // complete and lists no row, the answer that `board::path::not_found`
    // stands in for when the path is not in HEAD.
    let real = board.program().display().to_string();
    let empty = board.script(
        "empty-walk-board",
        &format!(
            r#"case "$*" in
  "log --json --limit 1 "*)
    printf '%s\n' '{{"schema":"board/log-report/v2","data":[],"truncated":false,"summary":{{"complete":true}}}}'
    exit 0;;
esac
exec "{real}" "$@"
"#
        ),
    );
    let walking = board.store_with(&empty, Limits::default());

    let recovered = publish(&walking, ContentFormat::Html, "two");

    assert_eq!(block_on(store.versions(&home())).unwrap(), [recovered]);
    assert_eq!(previous_in_tree(&board), serde_json::Value::Null);
}

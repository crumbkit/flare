//! How `BoardStore` checks page versions when `board log` cannot see the
//! whole history: past the 100,000 commits that one walk reads, and in a
//! shallow clone.
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
    Binding, BindingName, Blob, ContentFormat, Fills, MadeFrom, MoveError, Page, PageContent,
    PageName, PagePin, PageStore, PageStoreError, PageVersion, Principal, PrincipalId, Provenance,
    RecordPath, RecordType, Slot, SlotName, Slots, WriteProvenance,
};
use scratch::ScratchBoard;
use std::time::Duration;

/// One more commit than one `board log` walk reads.
const PAST_THE_WALK: usize = 100_001;

fn home() -> PageName {
    PageName::try_from("home").unwrap()
}

fn page(text: &str) -> Page {
    let slot = Slot::new(
        SlotName::try_from("tasks").unwrap(),
        RecordType::try_from("tasks/task/v2").unwrap(),
        Vec::new(),
        None,
    )
    .unwrap();
    Page::new(
        home(),
        PageContent::new(ContentFormat::Html, text).unwrap(),
        Slots::new(vec![slot]).unwrap(),
    )
}

fn ada() -> Principal {
    Principal::new(PrincipalId::try_from("ada").unwrap())
}

fn original() -> Provenance {
    Provenance::new(ada(), MadeFrom::Original, Vec::new()).unwrap()
}

fn publish(store: &BoardStore, text: &str) -> PageVersion {
    block_on(store.publish(page(text), original())).unwrap()
}

fn pin(version: &PageVersion) -> PagePin {
    PagePin::new(home(), version.clone())
}

fn view() -> BindingName {
    BindingName::try_from("my-view").unwrap()
}

/// A binding of `/tasks/a` to the home page at `version`, made from that
/// version.
fn binding(version: &PageVersion) -> Binding {
    let fills = Fills::new(vec![(
        SlotName::try_from("tasks").unwrap(),
        RecordPath::try_from("/tasks/a").unwrap(),
    )])
    .unwrap();
    let provenance = Provenance::new(ada(), MadeFrom::Page(pin(version)), Vec::new()).unwrap();
    Binding::new(view(), pin(version), fills, provenance)
}

fn mover() -> WriteProvenance {
    WriteProvenance::new(
        Blob::try_from("0".repeat(40).as_str()).unwrap(),
        ada(),
        view(),
    )
}

fn text_at(store: &BoardStore, version: &PageVersion) -> String {
    let published = block_on(store.at(&pin(version))).unwrap();
    published.page().content().text().to_owned()
}

/// A walk of 100,000 commits takes seconds, so the children here get two
/// minutes each.
fn slow_limits() -> Limits {
    let mut limits = Limits::default();
    limits.child_deadline = Duration::from_secs(120);
    limits
}

/// A binding of `/tasks/a` to the home page at `version`, made from
/// nothing, so saving it checks one pin.
fn original_binding(version: &PageVersion) -> Binding {
    let fills = Fills::new(vec![(
        SlotName::try_from("tasks").unwrap(),
        RecordPath::try_from("/tasks/a").unwrap(),
    )])
    .unwrap();
    Binding::new(view(), pin(version), fills, original())
}

// The one test that runs past board's real walk bound. Each walk of
// 100,000 commits takes several seconds on CI and a test is stopped after
// 60 seconds, so it runs one, in the save. The tests below that use a board
// program answering an incomplete walk cover moves and the other cases.
#[test]
fn pins_saves_and_fit_checks_keep_working_past_the_logs_walk() {
    let board = ScratchBoard::new();
    board.seed_record("/tasks/a", r#"{"$type":"tasks/task/v2","title":"first"}"#);
    let store = board.store_with(board.program(), slow_limits());
    let first = publish(&store, "one");
    publish(&store, "two");
    board.filler(PAST_THE_WALK);

    assert_eq!(text_at(&store, &first), "one");
    block_on(store.save_binding(original_binding(&first), &store)).unwrap();
    block_on(store.check_fit(&view(), &store)).unwrap();
    assert_eq!(
        block_on(store.binding(&view())).unwrap().pin().version(),
        &first
    );
}

#[test]
fn versions_above_a_shallow_boundary_keep_working_and_the_boundary_is_unavailable() {
    let origin = ScratchBoard::new();
    origin.seed_record("/tasks/a", r#"{"$type":"tasks/task/v2","title":"first"}"#);
    origin.commit_all("commit the board's configuration");
    let publisher = origin.store();
    let outside = publish(&publisher, "zero");
    let boundary = publish(&publisher, "one");
    let second = publish(&publisher, "two");
    let third = publish(&publisher, "three");

    let clone = origin.shallow_clone(3);
    let store = clone.store();
    assert_eq!(text_at(&store, &third), "three");
    assert_eq!(text_at(&store, &second), "two");
    assert_eq!(
        block_on(store.at(&pin(&boundary))),
        Err(PageStoreError::Unavailable)
    );
    assert_eq!(
        block_on(store.at(&pin(&outside))),
        Err(PageStoreError::NotFound)
    );
    assert_eq!(block_on(store.latest(&home())).unwrap().version(), &third);

    block_on(store.save_binding(binding(&second), &store)).unwrap();
    block_on(store.check_fit(&view(), &store)).unwrap();
    let fourth = publish(&store, "four");
    block_on(store.move_binding(&view(), &second, &fourth, &store, &mover())).unwrap();
    assert_eq!(
        block_on(store.binding(&view())).unwrap().pin().version(),
        &fourth
    );
}

#[test]
fn a_page_whose_newest_version_is_the_shallow_boundary_answers_unavailable() {
    let origin = ScratchBoard::new();
    origin.commit_all("commit the board's configuration");
    let publisher = origin.store();
    publish(&publisher, "one");
    let only = publish(&publisher, "two");

    let clone = origin.shallow_clone(1);
    let store = clone.store();
    assert_eq!(
        block_on(store.latest(&home())),
        Err(PageStoreError::Unavailable)
    );
    assert_eq!(
        block_on(store.at(&pin(&only))),
        Err(PageStoreError::Unavailable)
    );
    assert_eq!(
        block_on(store.publish(page("three"), original())),
        Err(PageStoreError::Unavailable)
    );
}

/// A board with versions "one" and "two" on main and one on a branch
/// `side`, checked out at main. Answers the board, its store and the
/// versions: first, second and the side one.
fn with_a_side_branch() -> (ScratchBoard, BoardStore, [PageVersion; 3]) {
    let board = ScratchBoard::new();
    board.seed_record("/tasks/a", r#"{"$type":"tasks/task/v2","title":"first"}"#);
    let store = board.store();
    let first = publish(&store, "one");
    let second = publish(&store, "two");
    board.git(&["checkout", "-q", "-b", "side"]);
    let side = publish(&store, "on a side branch");
    board.git(&["checkout", "-q", "main"]);
    (board, store, [first, second, side])
}

/// A board program that answers each walk of a log from HEAD whose
/// arguments start with `walk` with `answer`, and runs board for the rest.
fn answering_walks(board: &ScratchBoard, walk: &str, answer: &str) -> std::path::PathBuf {
    let real = board.program().display().to_string();
    board.script(
        "walk-board",
        &format!(
            r#"case "$*" in
  "{walk}"*)
    printf '%s\n' '{answer}'
    exit 0;;
esac
exec "{real}" "$@"
"#
        ),
    )
}

/// The answer of a walk that stopped before its end, with no row.
const STOPPED: &str =
    r#"{"schema":"board/log-report/v2","data":[],"truncated":true,"summary":{"complete":false}}"#;

#[test]
fn writes_refuse_a_pin_on_a_branch_that_head_cannot_reach() {
    let (_board, store, [first, _, side]) = with_a_side_branch();

    assert_eq!(
        block_on(store.save_binding(binding(&side), &store)),
        Err(PageStoreError::UnknownVersion)
    );
    assert_eq!(
        block_on(store.binding(&view())),
        Err(PageStoreError::NotFound)
    );
    block_on(store.save_binding(binding(&first), &store)).unwrap();
    assert_eq!(
        block_on(store.move_binding(&view(), &first, &side, &store, &mover())),
        Err(MoveError::Refused(PageStoreError::UnknownVersion))
    );
    let from_side = Provenance::new(ada(), MadeFrom::Page(pin(&side)), Vec::new()).unwrap();
    assert_eq!(
        block_on(store.publish(page("copied"), from_side)),
        Err(PageStoreError::UnresolvedOrigin)
    );
}

#[test]
fn writes_accept_a_pin_older_than_the_newest_on_heads_history() {
    let (_board, store, [first, second, _]) = with_a_side_branch();

    block_on(store.save_binding(binding(&first), &store)).unwrap();
    block_on(store.move_binding(&view(), &first, &second, &store, &mover())).unwrap();
    let from_first = Provenance::new(ada(), MadeFrom::Page(pin(&first)), Vec::new()).unwrap();
    block_on(store.publish(page("copied"), from_first)).unwrap();
}

#[test]
fn reading_a_version_checks_its_one_commit_and_walks_nothing_from_head() {
    let (board, store, [first, _, side]) = with_a_side_branch();
    block_on(store.save_binding(binding(&first), &store)).unwrap();
    let failing = board.script(
        "failing-walks",
        &format!(
            r#"case "$*" in
  "log --json --limit 1 "*|"log --json --all "*) exit 3;;
esac
exec "{}" "$@"
"#,
            board.program().display()
        ),
    );
    let reader = board.store_with(&failing, Limits::default());

    assert_eq!(text_at(&reader, &first), "one");
    block_on(reader.check_fit(&view(), &reader)).unwrap();
    // A read does not check that HEAD reaches the version; a pin is
    // checked when it is written.
    assert_eq!(text_at(&reader, &side), "on a side branch");
}

#[test]
fn a_pin_is_saved_and_moved_unchecked_when_the_newest_walk_from_head_stops_early() {
    let (board, _, [first, _, side]) = with_a_side_branch();
    let stopping = answering_walks(&board, "log --json --limit 1 ", STOPPED);
    let store = board.store_with(&stopping, Limits::default());

    // Accepted gap: a walk that stops before its end cannot decide.
    block_on(store.save_binding(binding(&first), &store)).unwrap();
    block_on(store.move_binding(&view(), &first, &side, &store, &mover())).unwrap();
    assert_eq!(
        block_on(store.binding(&view())).unwrap().pin().version(),
        &side
    );
}

#[test]
fn a_pin_is_saved_unchecked_when_the_full_walk_from_head_stops_before_the_version() {
    let (board, _, [_, _, side]) = with_a_side_branch();
    let stopping = answering_walks(&board, "log --json --all ", STOPPED);
    let store = board.store_with(&stopping, Limits::default());

    // Accepted gap: the newest row from HEAD is recent, but the walk of
    // every row stops before it would list the version.
    block_on(store.save_binding(binding(&side), &store)).unwrap();
}

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
    Binding, BindingName, Blob, ContentFormat, Fills, MadeFrom, Page, PageContent, PageName,
    PagePin, PageStore, PageStoreError, PageVersion, Principal, PrincipalId, Provenance,
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

#[test]
fn pins_saves_moves_and_fit_checks_keep_working_past_the_logs_walk() {
    let board = ScratchBoard::new();
    board.seed_record("/tasks/a", r#"{"$type":"tasks/task/v2","title":"first"}"#);
    let store = board.store_with(board.program(), slow_limits());
    let first = publish(&store, "one");
    let second = publish(&store, "two");
    board.filler(PAST_THE_WALK);

    assert_eq!(text_at(&store, &first), "one");
    block_on(store.save_binding(binding(&first), &store)).unwrap();
    block_on(store.check_fit(&view(), &store)).unwrap();
    block_on(store.move_binding(&view(), &first, &second, &store, &mover())).unwrap();
    let moved = block_on(store.binding(&view())).unwrap();
    assert_eq!(moved.pin().version(), &second);
    assert_eq!(block_on(store.bindings_of(&home())).unwrap(), [moved]);

    // Accepted gap: the list of every version and the newest version both
    // need the walk from HEAD to reach the page's last change.
    assert_eq!(
        block_on(store.versions(&home())),
        Err(PageStoreError::Unavailable)
    );
    assert_eq!(
        block_on(store.latest(&home())),
        Err(PageStoreError::Unavailable)
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

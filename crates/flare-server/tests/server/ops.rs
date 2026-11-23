//! `ops::publish` and `ops::bind`, the two writes that need no server.

use crate::harness::{PAGE, RECORD, TASK, provenance};
use flare_core::memory::{MemoryPageStore, MemoryRecordStore};
use flare_core::{
    BindingName, ContentFormat, Fills, MadeFrom, Page, PageContent, PageName, PageStore,
    PageStoreError, Principal, PrincipalId, RecordBody, RecordPath, RecordType, SlotName, Slots,
};
use flare_server::ops::{self, BindingError, PublishError};

fn page(template: &str) -> Page {
    Page::new(
        PageName::try_from(PAGE).unwrap(),
        PageContent::new(ContentFormat::Html, template).unwrap(),
        Slots::new(Vec::new()).unwrap(),
    )
}

fn fills(record: &str) -> Fills {
    Fills::new([(
        SlotName::try_from("task").unwrap(),
        RecordPath::try_from(record).unwrap(),
    )])
    .unwrap()
}

fn tester() -> Principal {
    Principal::new(PrincipalId::try_from("local:tester").unwrap())
}

#[tokio::test]
async fn publish_refuses_a_template_with_flaws_and_stores_nothing() {
    let pages = MemoryPageStore::new();
    let refused = ops::publish(
        &pages,
        page("<p>{{#open}}</p>"),
        provenance(MadeFrom::Original),
    )
    .await;
    assert!(
        matches!(refused, Err(PublishError::Flaws(_))),
        "{refused:?}"
    );
    assert_eq!(pages.pages().await.unwrap(), Vec::<PageName>::new());
    let version = ops::publish(&pages, page("<p>plain</p>"), provenance(MadeFrom::Original))
        .await
        .unwrap();
    assert_eq!(
        pages
            .latest(&PageName::try_from(PAGE).unwrap())
            .await
            .unwrap()
            .version(),
        &version
    );
}

#[tokio::test]
async fn bind_pins_the_newest_version_and_refuses_a_record_that_does_not_fit() {
    let pages = MemoryPageStore::new();
    let records = MemoryRecordStore::new();
    records
        .seed(
            RecordPath::try_from(RECORD).unwrap(),
            RecordType::try_from(TASK).unwrap(),
            RecordBody::try_from("{}").unwrap(),
        )
        .unwrap();
    ops::publish(&pages, page("<p>one</p>"), provenance(MadeFrom::Original))
        .await
        .unwrap();
    let misfit = ops::bind(
        &pages,
        &records,
        BindingName::try_from("kitchen").unwrap(),
        &PageName::try_from(PAGE).unwrap(),
        fills(RECORD),
        tester(),
    )
    .await;
    assert!(
        matches!(misfit, Err(BindingError::Store(PageStoreError::Misfit(_)))),
        "the page declares no slot task: {misfit:?}"
    );
    let unknown = ops::bind(
        &pages,
        &records,
        BindingName::try_from("kitchen").unwrap(),
        &PageName::try_from("no-page").unwrap(),
        fills(RECORD),
        tester(),
    )
    .await;
    assert!(matches!(
        unknown,
        Err(BindingError::Store(PageStoreError::NotFound))
    ));
}

#[tokio::test]
async fn bind_saves_a_binding_pinned_to_the_newest_version() {
    let server = crate::harness::Server::start().await;
    server.seed_page().await;
    let newest = server.seed_page().await;
    server.seed_record(RECORD, crate::harness::BODY);
    let binding = ops::bind(
        &server.pages,
        &server.records,
        BindingName::try_from("kitchen").unwrap(),
        &PageName::try_from(PAGE).unwrap(),
        fills(RECORD),
        tester(),
    )
    .await
    .unwrap();
    assert_eq!(binding.pin().version(), &newest);
    assert_eq!(binding.provenance().made_by(), &tester());
    let stored = server.pages.store.binding(binding.name()).await.unwrap();
    assert_eq!(stored.pin(), binding.pin());
}

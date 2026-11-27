//! The share store clause about the image table each version keeps.

use super::{ShareFixture, number, numbered_id};
use crate::contract::{Clause, Violation, ensure, fixture, unexpected};
use crate::{
    FileId, HtmlFile, ImagePath, MediaType, NewShare, NewVersion, ShareContent, ShareKind,
    ShareStore, Timestamp, VersionImage, VersionImages,
};

/// A table of `count` rows, whose file ids start at `first`.
fn table(first: u8, count: u8) -> Result<VersionImages, Violation> {
    let mut rows = Vec::new();
    for offset in 0..count {
        let byte = first.wrapping_add(offset);
        rows.push(VersionImage::new(
            fixture(ImagePath::try_from(format!("img/{byte}.png").as_str()))?,
            FileId::from_random([byte; 16]),
            fixture(MediaType::try_from("image/png"))?,
            u32::from(byte) * 100,
        ));
    }
    fixture(VersionImages::try_from(rows))
}

/// Version 1 written with two images, version 2 with one and version 3
/// with none each read back with the table they were written with.
///
/// # Errors
///
/// Returns [`Clause::EachVersionKeepsItsImageTable`] otherwise.
pub async fn each_version_keeps_its_image_table<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let id = numbered_id(0x31, 0);
    let first = table(1, 2)?;
    let second = table(9, 1)?;
    let new = contract_fixture
        .new_share(&id, &contract_fixture.owner, 10)
        .with_images(first.clone());
    unexpected(store.create(new).await)?;
    let two = contract_fixture.version(1, 20).with_images(second.clone());
    unexpected(store.publish(&id, two).await)?;
    unexpected(store.publish(&id, contract_fixture.version(2, 30)).await)?;
    let mut read = Vec::new();
    for value in 1..=3 {
        let version = unexpected(store.version(&id, number(value)?).await)?;
        read.push(version.images().clone());
    }
    let expected = [first, second, VersionImages::none()];
    ensure(
        read == expected,
        Clause::EachVersionKeepsItsImageTable,
        || format!("the versions read back with the tables {read:?}"),
    )
}

/// A share created with an HTML file reads back as the `html` kind with
/// that file and size, and a later Markdown version and a later HTML
/// version each read back with the content they were written with.
///
/// # Errors
///
/// Returns [`Clause::AnHtmlShareKeepsItsFile`] otherwise.
pub async fn an_html_share_keeps_its_file<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let id = numbered_id(0x32, 0);
    let page = |byte: u8, size: usize| -> Result<ShareContent, Violation> {
        fixture(HtmlFile::new(FileId::from_random([byte; 16]), size)).map(ShareContent::Html)
    };
    let new = NewShare::new(
        id.clone(),
        contract_fixture.owner.clone(),
        contract_fixture.title.clone(),
        page(1, 10)?,
        Timestamp::from_secs(10),
    );
    unexpected(store.create(new).await)?;
    let version = |content: ShareContent, at: u64| {
        NewVersion::new(
            content,
            contract_fixture.owner.clone(),
            Timestamp::from_secs(at),
        )
    };
    unexpected(
        store
            .publish(&id, version(contract_fixture.content(0).clone(), 20))
            .await,
    )?;
    unexpected(store.publish(&id, version(page(3, 4_194_304)?, 30)).await)?;
    let share = unexpected(store.get(&id).await)?;
    let mut read = Vec::new();
    for value in 1..=3 {
        let stored = unexpected(store.version(&id, number(value)?).await)?;
        read.push(stored.content().clone());
    }
    let expected = [
        page(1, 10)?,
        contract_fixture.content(0).clone(),
        page(3, 4_194_304)?,
    ];
    ensure(
        share.kind() == ShareKind::Html && read == expected,
        Clause::AnHtmlShareKeepsItsFile,
        || format!("the share is {share:?} and its versions read back as {read:?}"),
    )
}

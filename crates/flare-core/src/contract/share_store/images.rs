//! The share store clause about the image table each version keeps.

use super::{ShareFixture, number, numbered_id};
use crate::contract::{Clause, Violation, ensure, fixture, unexpected};
use crate::{FileId, ImagePath, MediaType, ShareStore, VersionImage, VersionImages};

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

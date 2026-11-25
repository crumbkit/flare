//! Unit tests of a version item's image table.

use super::{read_images, with_images};
use crate::dynamodb::items::{Item, ItemKind, Malformed, attr, text};
use aws_sdk_dynamodb::types::AttributeValue;
use flare_core::{FileId, ImagePath, MediaType, VersionImage, VersionImages};

fn table() -> VersionImages {
    VersionImages::try_from(vec![
        VersionImage::new(
            ImagePath::try_from("img/chart.png").unwrap(),
            FileId::from_random([1; 16]),
            MediaType::try_from("image/png").unwrap(),
            1234,
        ),
        VersionImage::new(
            ImagePath::try_from("photo.jpg").unwrap(),
            FileId::from_random([2; 16]),
            MediaType::try_from("image/jpeg").unwrap(),
            5,
        ),
    ])
    .unwrap()
}

#[test]
fn a_version_items_image_table_reads_back_in_its_order() {
    let item = with_images(Item::new(), &table());
    assert_eq!(read_images(&item), Ok(table()));
}

#[test]
fn a_version_item_stored_before_images_reads_as_having_none_and_an_empty_table_writes_nothing() {
    let item = with_images(Item::new(), &VersionImages::none());
    assert!(!item.contains_key(attr::IMAGES));
    assert_eq!(read_images(&item), Ok(VersionImages::none()));
}

#[test]
fn a_version_item_with_a_refused_row_is_malformed() {
    let malformed = Err(Malformed(ItemKind::Version));
    let mut wrong_type = Item::new();
    wrong_type.insert(attr::IMAGES.to_owned(), text("a.png"));
    assert_eq!(read_images(&wrong_type), malformed);
    let mut bad_path = with_images(Item::new(), &table());
    if let Some(AttributeValue::L(rows)) = bad_path.get_mut(attr::IMAGES)
        && let Some(AttributeValue::M(row)) = rows.first_mut()
    {
        row.insert("path".to_owned(), text("../etc/passwd"));
    }
    assert_eq!(read_images(&bad_path), malformed);
}

//! A version's image table, which its item holds as a list of maps under
//! `images`, each with the path, the file id, the media type and the size.
//! A version stored before images existed has no `images` attribute and
//! reads as having none.

use super::{Item, ItemKind, Malformed, attr, number, text};
use aws_sdk_dynamodb::types::AttributeValue;
use flare_core::{FileId, ImagePath, MediaType, VersionImage, VersionImages};
use std::collections::HashMap;

/// The names inside one row's map.
const PATH: &str = "path";
const FILE: &str = "file";
const MEDIA_TYPE: &str = "type";
const SIZE: &str = "size";

/// Adds the image table of a version to its item, when it has rows.
pub(super) fn with_images(mut item: Item, images: &VersionImages) -> Item {
    if images.is_empty() {
        return item;
    }
    let rows = images
        .rows()
        .iter()
        .map(|row| {
            AttributeValue::M(HashMap::from([
                (PATH.to_owned(), text(row.path().as_str())),
                (FILE.to_owned(), text(row.file().as_str())),
                (MEDIA_TYPE.to_owned(), text(row.media_type().as_str())),
                (SIZE.to_owned(), number(u64::from(row.size()))),
            ]))
        })
        .collect();
    item.insert(attr::IMAGES.to_owned(), AttributeValue::L(rows));
    item
}

/// The text value `name` of a row.
fn row_text<'a>(row: &'a HashMap<String, AttributeValue>, name: &str) -> Option<&'a str> {
    match row.get(name) {
        Some(AttributeValue::S(value)) => Some(value),
        _ => None,
    }
}

/// One row of the table, or `None` when it lacks a value or holds one its
/// type refuses.
fn read_row(value: &AttributeValue) -> Option<VersionImage> {
    let AttributeValue::M(row) = value else {
        return None;
    };
    let size = match row.get(SIZE) {
        Some(AttributeValue::N(size)) => size.parse::<u32>().ok()?,
        _ => return None,
    };
    Some(VersionImage::new(
        ImagePath::try_from(row_text(row, PATH)?).ok()?,
        FileId::try_from(row_text(row, FILE)?).ok()?,
        MediaType::try_from(row_text(row, MEDIA_TYPE)?).ok()?,
        size,
    ))
}

/// The image table a version's item holds; none when it has no `images`.
pub(super) fn read_images(item: &Item) -> Result<VersionImages, Malformed> {
    let kind = ItemKind::Version;
    let rows = match item.get(attr::IMAGES) {
        None => return Ok(VersionImages::none()),
        Some(AttributeValue::L(rows)) => rows,
        Some(_) => return Err(Malformed(kind)),
    };
    let rows = rows
        .iter()
        .map(read_row)
        .collect::<Option<Vec<_>>>()
        .ok_or(Malformed(kind))?;
    VersionImages::try_from(rows).map_err(|_| Malformed(kind))
}

#[cfg(test)]
mod tests;

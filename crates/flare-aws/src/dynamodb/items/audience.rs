//! The audience of an everyone grant, which its grant item and its listing
//! entry both hold.

use super::{Item, ItemKind, Malformed, attr, text};
use aws_sdk_dynamodb::types::AttributeValue;
use flare_core::{Audience, Grant};

/// Adds an everyone grant's audience to its item or its entry.
pub(super) fn with_audience(mut item: Item, grant: &Grant) -> Item {
    if let Some(audience) = grant.audience() {
        item.insert(attr::AUDIENCE.to_owned(), text(audience.as_str()));
    }
    item
}

/// The audience an item holds, or `None` when it holds none.
pub(super) fn read_audience(item: &Item, kind: ItemKind) -> Result<Option<Audience>, Malformed> {
    match item.get(attr::AUDIENCE) {
        None => Ok(None),
        Some(AttributeValue::S(audience)) => Audience::try_from(audience.as_str())
            .map(Some)
            .map_err(|_| Malformed(kind)),
        Some(_) => Err(Malformed(kind)),
    }
}

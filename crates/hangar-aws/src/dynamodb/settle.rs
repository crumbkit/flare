//! The consistent reads that settle a write whose outcome is unknown.
//!
//! Each read compares what the table holds with what the write would have
//! written, on the fields that only that write sets: a create's owner and
//! creation time, a publish's publisher, time and text, a grant's `rev`,
//! and, for a revoke, the grant's absence, which the revoke of another
//! admin does not accept.

use crate::dynamodb::Table;
use crate::dynamodb::items::{Item, ItemKind, Malformed, Rev, rev_of, share_from, version_from};
use crate::dynamodb::keys::Key;
use crate::dynamodb::retry::Settled;
use hangar_core::{Grant, PrincipalId, Share, ShareId, ShareVersion, SharedVersion};

/// Logs an item that the store cannot read, by its kind alone.
pub(crate) fn malformed(Malformed(kind): Malformed) {
    tracing::warn!(item = kind.name(), "a stored item is malformed");
}

/// Reads the item at `key` and settles it with `matches`.
async fn compare<T>(
    table: &Table,
    key: &Key,
    kind: ItemKind,
    matches: impl FnOnce(Option<&Item>) -> Result<Option<T>, Malformed>,
) -> Settled<T> {
    let Ok(item) = table.get(key, kind).await else {
        return Settled::Unavailable;
    };
    match matches(item.as_ref()) {
        Ok(Some(done)) => Settled::Matches(done),
        Ok(None) => Settled::Differs,
        Err(error) => {
            malformed(error);
            Settled::Unavailable
        }
    }
}

/// Whether the table holds the share that a create of `share` writes.
pub(crate) async fn created(table: &Table, share: &Share) -> Settled<Share> {
    compare(table, &Key::share(share.id()), ItemKind::Share, |item| {
        let Some(item) = item else { return Ok(None) };
        let held = share_from(item, share.id())?;
        let same = held.owner() == share.owner() && held.created() == share.created();
        Ok(same.then(|| share.clone()))
    })
    .await
}

/// Whether the table holds `version` as a publish writes it.
pub(crate) async fn published(table: &Table, version: &SharedVersion) -> Settled<ShareVersion> {
    let key = Key::version(version.share(), version.number());
    compare(table, &key, ItemKind::Version, |item| {
        let Some(item) = item else { return Ok(None) };
        let held = version_from(item, version.share(), version.number())?;
        Ok((held == *version).then_some(version.number()))
    })
    .await
}

/// Whether the table holds the grant item that a grant with `rev` writes.
pub(crate) async fn granted(table: &Table, grant: &Grant, rev: &Rev) -> Settled<Grant> {
    let key = Key::grant(grant.share(), grant.grantee());
    compare(table, &key, ItemKind::Grant, |item| {
        let same = item.and_then(rev_of) == Some(rev.as_str());
        Ok(same.then(|| grant.clone()))
    })
    .await
}

/// Whether the table holds no grant of `grantee` on `share`, as a revoke
/// leaves it.
pub(crate) async fn revoked(table: &Table, share: &ShareId, grantee: &PrincipalId) -> Settled<()> {
    let key = Key::grant(share, grantee);
    compare(table, &key, ItemKind::Grant, |item| {
        Ok(item.is_none().then_some(()))
    })
    .await
}

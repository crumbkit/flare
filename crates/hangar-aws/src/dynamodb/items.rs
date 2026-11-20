//! The table's items: shares, versions, grants and list entries, to and
//! from DynamoDB's attribute maps.
//!
//! A read item that lacks an attribute, or holds one that its type refuses,
//! is [`Malformed`], and the store answers unavailable for it.

use crate::dynamodb::keys::{Key, entry_of, grantee_of};
use aws_sdk_dynamodb::types::AttributeValue;
use hangar_core::{
    DocumentText, Grant, PrincipalId, Role, Share, ShareContent, ShareId, ShareKind, ShareTitle,
    ShareVersion, SharedVersion, Timestamp,
};
use std::collections::HashMap;

/// One item as DynamoDB holds it.
pub(crate) type Item = HashMap<String, AttributeValue>;

/// The attribute names, which every expression reaches through a
/// placeholder.
pub(crate) mod attr {
    pub(crate) const PK: &str = "pk";
    pub(crate) const SK: &str = "sk";
    pub(crate) const OWNER: &str = "owner";
    pub(crate) const KIND: &str = "kind";
    pub(crate) const TITLE: &str = "title";
    pub(crate) const CREATED: &str = "created";
    pub(crate) const LATEST: &str = "latest";
    pub(crate) const TEXT: &str = "text";
    pub(crate) const PUBLISHED_BY: &str = "published_by";
    pub(crate) const ROLE: &str = "role";
    pub(crate) const GRANTED_BY: &str = "granted_by";
    pub(crate) const REV: &str = "rev";
    pub(crate) const COUNT: &str = "n";
}

/// The kinds of item, which a log line names in place of the item's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemKind {
    Share,
    Version,
    Grant,
    OwnerEntry,
    GranteeEntry,
    Object,
}

impl ItemKind {
    /// The kind's name in a log line.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Share => "share",
            Self::Version => "version",
            Self::Grant => "grant",
            Self::OwnerEntry => "owner entry",
            Self::GranteeEntry => "grantee entry",
            Self::Object => "object",
        }
    }
}

/// An item of this kind lacks an attribute or holds a refused value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Malformed(pub(crate) ItemKind);

/// The value of a grant's `rev`: 128 random bits as 32 hexadecimal
/// characters, new on every put of the grant item, so a write conditioned
/// on it applies only to the grant the writer read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rev(String);

impl Rev {
    /// A new random `rev`, or `None` when the system has no randomness.
    pub(crate) fn fresh() -> Option<Self> {
        crate::dynamodb::random_hex().map(Self)
    }

    /// The `rev` that 16 bytes give.
    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(crate::dynamodb::hex(&bytes))
    }

    /// The `rev` as text.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// A grant as the table holds it, with its `rev`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Held {
    pub(crate) grant: Grant,
    pub(crate) rev: Rev,
}

pub(crate) fn text(value: &str) -> AttributeValue {
    AttributeValue::S(value.to_owned())
}

pub(crate) fn number(value: u64) -> AttributeValue {
    AttributeValue::N(value.to_string())
}

fn keyed(key: &Key, attributes: impl IntoIterator<Item = (&'static str, AttributeValue)>) -> Item {
    let mut item = Item::from([
        (attr::PK.to_owned(), text(&key.pk)),
        (attr::SK.to_owned(), text(&key.sk)),
    ]);
    item.extend(
        attributes
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value)),
    );
    item
}

/// The name of a kind of share, or `None` for a kind this crate does not
/// store yet.
fn kind_name(kind: ShareKind) -> Option<&'static str> {
    match kind {
        ShareKind::Document => Some("document"),
        _ => None,
    }
}

pub(crate) fn role_name(role: Role) -> &'static str {
    match role {
        Role::Reader => "reader",
        Role::Admin => "admin",
    }
}

/// Reads the text attribute `name`.
fn read_text<'a>(item: &'a Item, name: &str, kind: ItemKind) -> Result<&'a str, Malformed> {
    match item.get(name) {
        Some(AttributeValue::S(value)) => Ok(value),
        _ => Err(Malformed(kind)),
    }
}

/// Reads the number attribute `name`.
fn read_number(item: &Item, name: &str, kind: ItemKind) -> Result<u64, Malformed> {
    match item.get(name) {
        Some(AttributeValue::N(value)) => value.parse().map_err(|_| Malformed(kind)),
        _ => Err(Malformed(kind)),
    }
}

fn read_principal(item: &Item, name: &str, kind: ItemKind) -> Result<PrincipalId, Malformed> {
    PrincipalId::try_from(read_text(item, name, kind)?).map_err(|_| Malformed(kind))
}

fn read_time(item: &Item, kind: ItemKind) -> Result<Timestamp, Malformed> {
    read_number(item, attr::CREATED, kind).map(Timestamp::from_secs)
}

fn read_kind(item: &Item, kind: ItemKind) -> Result<ShareKind, Malformed> {
    match read_text(item, attr::KIND, kind)? {
        "document" => Ok(ShareKind::Document),
        _ => Err(Malformed(kind)),
    }
}

fn read_title(item: &Item, kind: ItemKind) -> Result<ShareTitle, Malformed> {
    ShareTitle::try_from(read_text(item, attr::TITLE, kind)?).map_err(|_| Malformed(kind))
}

fn read_latest(item: &Item, kind: ItemKind) -> Result<ShareVersion, Malformed> {
    let latest = u32::try_from(read_number(item, attr::LATEST, kind)?);
    latest
        .ok()
        .and_then(|latest| ShareVersion::try_from(latest).ok())
        .ok_or(Malformed(kind))
}

/// The role in an item, or `None` when it has none it can read.
pub(crate) fn role_of(item: &Item) -> Option<Role> {
    match item.get(attr::ROLE) {
        Some(AttributeValue::S(role)) if role == "reader" => Some(Role::Reader),
        Some(AttributeValue::S(role)) if role == "admin" => Some(Role::Admin),
        _ => None,
    }
}

fn read_role(item: &Item, kind: ItemKind) -> Result<Role, Malformed> {
    role_of(item).ok_or(Malformed(kind))
}

/// A share's item, or `None` for a kind of share this crate does not store.
pub(crate) fn share_item(share: &Share) -> Option<Item> {
    Some(keyed(
        &Key::share(share.id()),
        [
            (attr::OWNER, text(share.owner().as_str())),
            (attr::KIND, text(kind_name(share.kind())?)),
            (attr::TITLE, text(share.title().as_str())),
            (attr::CREATED, number(share.created().as_secs())),
            (attr::LATEST, number(u64::from(share.latest().get()))),
        ],
    ))
}

/// The share that the item of share `id` holds.
pub(crate) fn share_from(item: &Item, id: &ShareId) -> Result<Share, Malformed> {
    let kind = ItemKind::Share;
    Ok(Share::new(
        id.clone(),
        read_principal(item, attr::OWNER, kind)?,
        read_kind(item, kind)?,
        read_title(item, kind)?,
        read_time(item, kind)?,
        read_latest(item, kind)?,
    ))
}

/// A version's item, or `None` for a kind of content this crate does not
/// store.
pub(crate) fn version_item(version: &SharedVersion) -> Option<Item> {
    let (kind, body) = match version.content() {
        ShareContent::Document(text) => (ShareKind::Document, text.as_str()),
        _ => return None,
    };
    Some(keyed(
        &Key::version(version.share(), version.number()),
        [
            (attr::KIND, text(kind_name(kind)?)),
            (attr::TEXT, text(body)),
            (attr::PUBLISHED_BY, text(version.published_by().as_str())),
            (attr::CREATED, number(version.created().as_secs())),
        ],
    ))
}

/// The version `number` of share `id` that its item holds.
pub(crate) fn version_from(
    item: &Item,
    id: &ShareId,
    number: ShareVersion,
) -> Result<SharedVersion, Malformed> {
    let kind = ItemKind::Version;
    let content = match read_kind(item, kind)? {
        ShareKind::Document => DocumentText::try_from(read_text(item, attr::TEXT, kind)?)
            .map(ShareContent::Document)
            .map_err(|_| Malformed(kind))?,
        _ => return Err(Malformed(kind)),
    };
    Ok(SharedVersion::new(
        id.clone(),
        number,
        content,
        read_principal(item, attr::PUBLISHED_BY, kind)?,
        read_time(item, kind)?,
    ))
}

/// A share's entry in its owner's list, or `None` for a kind of share this
/// crate does not store.
pub(crate) fn owner_entry_item(share: &Share) -> Option<Item> {
    Some(keyed(
        &Key::owner_entry(share.owner(), share.created(), share.id()),
        [
            (attr::KIND, text(kind_name(share.kind())?)),
            (attr::TITLE, text(share.title().as_str())),
            (attr::CREATED, number(share.created().as_secs())),
            (attr::LATEST, number(u64::from(share.latest().get()))),
        ],
    ))
}

/// The share that an entry in `owner`'s list holds.
pub(crate) fn owner_entry_from(item: &Item, owner: &PrincipalId) -> Result<Share, Malformed> {
    let kind = ItemKind::OwnerEntry;
    let (created, id) = entry_of(read_text(item, attr::SK, kind)?).ok_or(Malformed(kind))?;
    Ok(Share::new(
        id,
        owner.clone(),
        read_kind(item, kind)?,
        read_title(item, kind)?,
        created,
        read_latest(item, kind)?,
    ))
}

/// A grant's item, with its `rev`.
pub(crate) fn grant_item(grant: &Grant, rev: &Rev) -> Item {
    keyed(
        &Key::grant(grant.share(), grant.grantee()),
        [
            (attr::ROLE, text(role_name(grant.role()))),
            (attr::GRANTED_BY, text(grant.granted_by().as_str())),
            (attr::CREATED, number(grant.created().as_secs())),
            (attr::REV, text(rev.as_str())),
        ],
    )
}

/// The grant on share `id` that a grant item holds, with its `rev`.
pub(crate) fn grant_from(item: &Item, id: &ShareId) -> Result<Held, Malformed> {
    let kind = ItemKind::Grant;
    let grantee = grantee_of(read_text(item, attr::SK, kind)?).ok_or(Malformed(kind))?;
    let grant = Grant::new(
        id.clone(),
        grantee,
        read_role(item, kind)?,
        read_principal(item, attr::GRANTED_BY, kind)?,
        read_time(item, kind)?,
    );
    let rev = Rev(read_text(item, attr::REV, kind)?.to_owned());
    Ok(Held { grant, rev })
}

/// A grant's entry in its grantee's list.
pub(crate) fn grantee_entry_item(grant: &Grant) -> Item {
    keyed(
        &Key::grantee_entry(grant.grantee(), grant.created(), grant.share()),
        [
            (attr::ROLE, text(role_name(grant.role()))),
            (attr::GRANTED_BY, text(grant.granted_by().as_str())),
            (attr::CREATED, number(grant.created().as_secs())),
        ],
    )
}

/// The grant that an entry in `grantee`'s list holds.
pub(crate) fn grantee_entry_from(item: &Item, grantee: &PrincipalId) -> Result<Grant, Malformed> {
    let kind = ItemKind::GranteeEntry;
    let (created, id) = entry_of(read_text(item, attr::SK, kind)?).ok_or(Malformed(kind))?;
    Ok(Grant::new(
        id,
        grantee.clone(),
        read_role(item, kind)?,
        read_principal(item, attr::GRANTED_BY, kind)?,
        created,
    ))
}

/// The `rev` in a grant item, or `None` when it has none.
pub(crate) fn rev_of(item: &Item) -> Option<&str> {
    match item.get(attr::REV) {
        Some(AttributeValue::S(rev)) => Some(rev),
        _ => None,
    }
}

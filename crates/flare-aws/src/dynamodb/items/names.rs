//! The names in the table's items: each attribute, and each kind of item.

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
    pub(crate) const PORT: &str = "port";
    pub(crate) const EXPIRES: &str = "expires";
    /// The attribute that the table's TTL reads, which login codes,
    /// sessions with their links and guards, and old sealing keys carry.
    pub(crate) const TTL: &str = "expires_at";
    pub(crate) const USED_BY: &str = "used_by";
    pub(crate) const DIGEST: &str = "digest";
    pub(crate) const LABEL: &str = "label";
    pub(crate) const REVOKED: &str = "revoked";
    pub(crate) const TOKEN: &str = "token";
    pub(crate) const RESPONSE: &str = "response";
    pub(crate) const FLARE: &str = "flare";
    pub(crate) const VERSION: &str = "version";
    pub(crate) const POSITION: &str = "position";
    pub(crate) const PRINCIPAL: &str = "principal";
    pub(crate) const EMAIL: &str = "email";
    pub(crate) const PERSON: &str = "person";
    pub(crate) const BYTES: &str = "bytes";
    pub(crate) const COMMENT: &str = "comment";
    pub(crate) const PARENT: &str = "parent";
    pub(crate) const AUTHOR: &str = "author";
    pub(crate) const BODY: &str = "body";
    pub(crate) const EDITED: &str = "edited";
    pub(crate) const ACTION: &str = "action";
    /// The audience of an everyone grant, on its grant item and its entry.
    pub(crate) const AUDIENCE: &str = "audience";
    /// A team's state, `active` or `deleting`.
    pub(crate) const STATE: &str = "state";
    /// A version's image table: a list of maps.
    pub(crate) const IMAGES: &str = "images";
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
    Code,
    Token,
    LabelSlot,
    Link,
    Sequence,
    Session,
    SessionLink,
    SessionGuard,
    SealingKey,
    Comment,
    CommentPointer,
    CommentEvent,
    CommentKeySlot,
    Cursor,
    Team,
    Member,
    IndexEntry,
    IndexMarker,
    Identity,
    TeamsMarker,
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
            Self::Code => "login code",
            Self::Token => "token",
            Self::LabelSlot => "token label",
            Self::Link => "response link",
            Self::Sequence => "response sequence",
            Self::Session => "session",
            Self::SessionLink => "session link",
            Self::SessionGuard => "session guard",
            Self::SealingKey => "sealing key",
            Self::Comment => "comment",
            Self::CommentPointer => "comment pointer",
            Self::CommentEvent => "comment event",
            Self::CommentKeySlot => "comment key slot",
            Self::Cursor => "cursor",
            Self::Team => "team",
            Self::Member => "team member",
            Self::IndexEntry => "activity index entry",
            Self::IndexMarker => "activity index marker",
            Self::Identity => "identity marker",
            Self::TeamsMarker => "teams marker",
        }
    }
}

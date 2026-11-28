//! Types, traits and contract tests that every flare crate shares.
//!
//! Every type with a rule is built only by a parsing constructor that
//! returns a typed error, so the rest of a program never holds an unchecked
//! value. The traits are [`Authenticator`], [`PageStore`], [`RecordStore`],
//! [`ShareStore`], [`GrantStore`] and [`FileStore`]. [`memory`] holds
//! in-memory stores for tests, and [`contract`] holds one suite per trait
//! that each implementation runs from its own tests.
//!
//! A share is a document that one owner publishes in numbered versions and
//! opens to the people a grant names. [`Shares`] decides every request
//! about a share from the caller's [`Person`], their verified address or
//! their username as the deployment's [`IdentityKind`] chooses, the share
//! and its grants, and is the only caller of the share and grant stores.
//! [`Flares`] wraps it with the creators rule and the response links that
//! `--re` writes, and reads the responses to a flare since a cursor.
//! [`TokenStore`] keeps the digests of CLI tokens and login codes.
//! [`Teams`] reads the teams a share can be granted to, [`FlareTeams`]
//! keeps them in a [`TeamStore`], and [`DocumentTeams`] reads them, read
//! only, from a teams document that the deployment writes.

mod auth;
mod binding;
mod comment;
pub mod contract;
mod email;
mod error;
mod file;
mod flares;
mod grant;
pub mod memory;
mod name;
mod page;
mod person;
mod provenance;
mod record;
mod response;
mod sealing;
mod session;
mod share;
mod sharing;
mod store;
mod teams;
mod time;
mod token;

pub use auth::{Access, AuthError, Authenticator, Credential, Decision, Principal, Resource};
pub use binding::Binding;
pub use comment::{
    Anchor, Comment, CommentAction, CommentBody, CommentEvent, CommentId, CommentKey, CursorName,
    MAX_COMMENT_CHARS, MAX_COMMENTS_PER_FLARE, MAX_CONTEXT_CHARS, MAX_CURSORS_PER_PERSON,
    MAX_QUOTE_CHARS, MIN_QUOTE_CHARS, NewComment, Quote, QuoteContext,
};
pub use email::Email;
pub use error::{ParseError, Subject};
pub use file::{
    FileBytes, FileId, ImageKind, ImagePath, MAX_FILE_BYTES, MAX_IMAGE_PATH, MAX_IMAGES,
    MAX_SWEPT_FILES, MAX_UPLOAD_BYTES, MediaType, NewFile, SNIFF_BYTES, SWEEP_AGE, StoredFile,
    SweepReport, VersionImage, VersionImages, sweep_files,
};
pub use flares::{
    ActivityItem, ActivityPage, AllAdmitted, ByIdentity, CommentActivity, CommentDraft,
    CommentThread, CreatorDomain, CreatorList, Creators, CreatorsError, FlareError, Flared, Flares,
    MAX_EXAMINED, MAX_FLARES, MAX_SWEPT, NamedRecipients, NewFlare, Posted, RecipientError,
    Recipients, ResponseActivity, Revoked, TeamDeletion,
};
pub use flares::{Found, MAX_HITS, SearchError, SearchHit};
pub use grant::{Grant, GrantRevoke, GrantWrite, Granter, MAX_GRANTS, Revoker, Role};
pub use name::{BindingName, FieldName, PageName, PageVersion, PrincipalId, SlotName};
pub use page::{
    ContentFormat, Fills, Misfit, Page, PageContent, PagePin, PublishedPage, Sample, Slot, Slots,
};
pub use person::{IdentityKind, Person, Username};
pub use provenance::{MadeFrom, Provenance, RecordRef, WriteProvenance};
pub use record::{Blob, MAX_RECORD_BYTES, Record, RecordBody, RecordPath, RecordType};
pub use response::{IndexEntry, MAX_PAGE, NewEntry, NewLink, Position, Reader, ResponseLink};
pub use sealing::{
    KEY_GRACE, KEY_ROTATE_AFTER, KeyBytes, KeyId, SealingKey, SealingKeys, SealingKeysError,
};
pub use session::{
    MAX_SESSIONS_PER_PERSON, NewSession, SESSION_CAP, SESSION_IDLE, SESSION_REFRESH, SessionRule,
    SessionStore, SessionStoreError,
};
pub use share::{
    DocumentText, HtmlFile, Listing, MAX_DOCUMENT, MAX_HTML, MAX_LISTED, MAX_SHARES_PER_OWNER,
    MAX_TITLE, MAX_VERSIONS, NewShare, NewVersion, Share, ShareContent, ShareId, ShareKind,
    ShareTitle, ShareVersion, SharedVersion,
};
pub use sharing::{ListedGrant, Opened, ShareError, SharedWithMe, Shares, Standing};
pub use store::{
    Added, CommentStore, CommentStoreError, CursorStore, FileCursor, FilePage, FileStore,
    FileStoreError, FillTypes, GrantStore, GrantStoreError, ListKey, ListPage, ListedFile,
    MAX_FILE_PAGE, MoveError, PageStore, PageStoreError, ReadError, RecordStore, Rejection,
    ResponseStore, ResponseStoreError, ShareStore, ShareStoreError, TokenStore, TokenStoreError,
    WriteError,
};
pub use teams::{
    Admission, Audience, DOCUMENT_MAX_AGE, DocumentRefused, DocumentTeams, DocumentUnreadable,
    FlareTeams, Group, MAX_DIRECTORY_MEMBERS, MAX_DIRECTORY_TEAMS, MAX_DOCUMENT_BYTES, MAX_MEMBERS,
    MAX_TEAMS_JOINED, MAX_TEAMS_OWNED, MailDomain, ManagedBy, NoTeams, Team, TeamName, TeamState,
    TeamStore, TeamStoreError, TeamView, Teams, TeamsDocument, TeamsError,
};
pub use time::Timestamp;
pub use token::{
    CODE_LIFE, Digest, LoginCode, NewToken, StoredToken, TOKEN_LIFE, TokenId, TokenLabel, UsedCode,
};

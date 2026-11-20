//! Types, traits and contract tests that every hangar crate shares.
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
//! about a share from the caller's verified [`Email`], the share and its
//! grants, and is the only caller of the share and grant stores.

mod auth;
mod binding;
pub mod contract;
mod email;
mod error;
mod file;
mod grant;
pub mod memory;
mod name;
mod page;
mod provenance;
mod record;
mod share;
mod sharing;
mod store;
mod time;

pub use auth::{Access, AuthError, Authenticator, Credential, Decision, Principal, Resource};
pub use binding::Binding;
pub use email::Email;
pub use error::{ParseError, Subject};
pub use file::{FileBytes, FileId, MAX_FILE_BYTES, MediaType, NewFile, StoredFile};
pub use grant::{Grant, GrantRevoke, GrantWrite, Granter, MAX_GRANTS, Revoker, Role};
pub use name::{BindingName, FieldName, PageName, PageVersion, PrincipalId, SlotName};
pub use page::{
    ContentFormat, Fills, Misfit, Page, PageContent, PagePin, PublishedPage, Sample, Slot, Slots,
};
pub use provenance::{MadeFrom, Provenance, RecordRef, WriteProvenance};
pub use record::{Blob, MAX_RECORD_BYTES, Record, RecordBody, RecordPath, RecordType};
pub use share::{
    DocumentText, Listing, MAX_LISTED, MAX_SHARES_PER_OWNER, MAX_VERSIONS, NewShare, NewVersion,
    Share, ShareContent, ShareId, ShareKind, ShareTitle, ShareVersion, SharedVersion,
};
pub use sharing::{Opened, ShareError, SharedWithMe, Shares, Standing};
pub use store::{
    FileStore, FileStoreError, FillTypes, GrantStore, GrantStoreError, MoveError, PageStore,
    PageStoreError, ReadError, RecordStore, Rejection, ShareStore, ShareStoreError, WriteError,
};
pub use time::Timestamp;

//! The request, response and problem types of flare's JSON API, which the
//! server answers with and the `flare` command reads, and their JSON
//! Schema.
//!
//! `schema/flare-api.schema.json` at the repository root commits the
//! schema that [`schema::generate`] writes, and a test fails when the two
//! differ, so a client cannot drift from the server unseen.

mod cursor;
mod groups;
mod problem;
pub mod schema;
mod search;
mod time;
mod wire;

pub use cursor::{Cursor, CursorError, ListScope, ShareCursor};
pub use groups::{
    GrantEntry, GrantList, GrantState, GranteeKind, MembersChanged, MembersRequest, RevokeRequest,
    Revoked, TeamDeleted, TeamList, TeamManagedBy, TeamRequest, TeamRole, TeamShown, TeamSummary,
    TransferRequest,
};
pub use problem::{ApiCode, ProblemDocument, Recovery};
pub use search::{
    FieldKind, SearchAnswer, SearchError, SearchField, SearchFields, SearchPart, SearchSpan,
    SearchValue,
};
pub use time::utc;
pub use wire::{
    ActivityItem, ActivityPage, Anchor, Comment, CommentEdit, CommentOrReply, CommentThread,
    ExchangeRequest, FlareRef, Format, GrantRequest, NamedCursor, NewComment, OpenedShare, Reply,
    Role, ShareList, ShareRequest, ShareSummary, Shared, TokenIssued, VersionRequest,
};

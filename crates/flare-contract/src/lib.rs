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
mod time;
mod wire;

pub use cursor::{Cursor, CursorError};
pub use groups::{
    GrantEntry, GrantList, GrantState, GranteeKind, MembersChanged, MembersRequest, RevokeRequest,
    Revoked, TeamDeleted, TeamList, TeamRequest, TeamRole, TeamShown, TeamSummary,
};
pub use problem::{ApiCode, ProblemDocument, Recovery};
pub use time::utc;
pub use wire::{
    ActivityItem, ActivityPage, ExchangeRequest, FlareRef, Format, GrantRequest, OpenedShare, Role,
    ShareList, ShareRequest, ShareSummary, Shared, TokenIssued, VersionRequest,
};

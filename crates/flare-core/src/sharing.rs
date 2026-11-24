//! Who may do what to a share: the decision tables and the [`Shares`]
//! service that applies them.
//!
//! A request decides in two steps. [`decide_actor`] runs on the actor's
//! standing alone: a stranger gets `NotFound` for everything, and a reader
//! gets `Forbidden` for everything except reading and leaving, before
//! anything about the target is read. Only a grant or a revoke by the
//! owner or an admin goes on to read the target's grant and run
//! [`decide_target`].
//!
//! | Action | Owner | Admin | Reader | Stranger |
//! | --- | --- | --- | --- | --- |
//! | Read a version or a file | yes | yes | yes | not found |
//! | List the grants | yes | yes | forbidden | not found |
//! | Publish | yes | forbidden | forbidden | not found |
//! | Grant Reader to no grant or a Reader | yes | yes | forbidden | not found |
//! | Grant Admin, or make an Admin a Reader | yes | forbidden | forbidden | not found |
//! | Revoke a Reader | yes | yes | forbidden | not found |
//! | Revoke an Admin | yes | forbidden | forbidden | not found |
//! | Leave | owner cannot be changed | yes | yes | not found |
//! | Grant to or revoke the owner | owner cannot be changed | owner cannot be changed | forbidden | not found |
//!
//! An admin cannot touch another admin, so one admin cannot lock the owner
//! out or make more admins.
//!
//! # Lifecycle of a share's versions
//!
//! | State | Event | Next state | Who |
//! | --- | --- | --- | --- |
//! | no share | create with a free id | latest = 1 | the owner |
//! | no share | create with a taken id | nothing written, `IdTaken`; the first share is unchanged | the owner |
//! | owner has 500 shares | create | nothing written, `TooManyShares` | the owner |
//! | latest = n, n < 100 | publish | latest = n + 1, version n + 1 stored | the owner |
//! | latest = 100 | publish | unchanged, `TooManyVersions` | the owner |
//! | latest = n | a store loses a race on latest | the store reads again and retries a bounded number of times, then `Unavailable`, and the outcome is unknown | the store |
//!
//! The version number only grows, and every version stays readable. A share
//! is never deleted.
//!
//! # Lifecycle of a person's grant on a share
//!
//! | State | Event | Next state | Who |
//! | --- | --- | --- | --- |
//! | none | grant Reader or Admin, expecting none | Reader or Admin | the owner, or an admin for Reader |
//! | Reader | grant Admin, expecting Reader | Admin, time of first grant kept | the owner |
//! | Admin | grant Reader, expecting Admin | Reader, time of first grant kept | the owner |
//! | Reader or Admin | revoke, expecting that role | none | the owner, or an admin for Reader |
//! | Reader or Admin | the person leaves, expecting their role | none | the grantee |
//! | any | a write whose expected role or grant item is not what the store read | unchanged, `GrantChanged` | the caller reads again |
//! | any | a write under an admin whose Admin grant has ended | unchanged, `Forbidden` | none |
//! | none | a grant that would be the share's 257th | unchanged, `TooManyGrants` | none |
//! | any | a store whose own write a concurrent writer cancels | the store retries at least three times, then `Unavailable`, and the outcome is unknown | the store |
//!
//! The owner is a field of the share and never holds a grant, so no event
//! changes the owner.

mod error;
mod service;

pub use error::ShareError;
pub use service::{ListedGrant, Opened, SharedWithMe, Shares};

use crate::grant::Role;

/// What a principal is to one share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Standing {
    /// The principal's verified address is the share's owner.
    Owner,
    /// The principal holds an Admin grant.
    Admin,
    /// The principal holds a Reader grant.
    Reader,
    /// The principal has no standing in the share.
    Stranger,
}

/// What a request does to a share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShareAction {
    /// Read a version or a file.
    Read,
    /// List the share's grants.
    ListGrants,
    /// Publish a version.
    Publish,
    /// Grant a role to another person.
    Grant,
    /// Revoke another person's grant.
    Revoke,
    /// End one's own grant.
    Leave,
}

/// What a grant or a revoke does to its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetAction {
    /// Give the target this role.
    Grant(Role),
    /// End the target's grant.
    Revoke,
}

/// What the target of a grant or a revoke is to the share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    /// The share's owner.
    Owner,
    /// Anyone else, with the role their grant holds, if any.
    Person(Option<Role>),
}

/// Why a request is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The actor has no standing, and learns nothing about the share.
    NotFound,
    /// The actor has a standing and lacks the right.
    Forbidden,
    /// The target is the owner, whom no grant or revoke changes.
    OwnerCannotBeChanged,
    /// A revoke names a person with no grant.
    GrantNotFound,
}

/// The first step's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// The request may go ahead.
    Allow,
    /// The request depends on the target, so read it and run
    /// [`decide_target`] under this authority.
    ReadTarget(Authority),
    /// The request is refused.
    Refuse(Refusal),
}

/// The standing under which an actor grants or revokes: the owner, or an
/// admin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Authority {
    /// The actor owns the share.
    Owner,
    /// The actor holds an Admin grant.
    Admin,
}

/// The second step's answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    /// The request may go ahead.
    Allow,
    /// The request is refused.
    Refuse(Refusal),
}

/// Decides a request from the actor's standing alone.
pub(crate) fn decide_actor(standing: Standing, action: ShareAction) -> Step {
    use ShareAction::{Grant, Leave, ListGrants, Publish, Read, Revoke};
    match (standing, action) {
        (Standing::Stranger, _) => Step::Refuse(Refusal::NotFound),
        (Standing::Reader, Read | Leave)
        | (Standing::Admin, Read | ListGrants | Leave)
        | (Standing::Owner, Read | ListGrants | Publish) => Step::Allow,
        (Standing::Reader, ListGrants | Publish | Grant | Revoke) | (Standing::Admin, Publish) => {
            Step::Refuse(Refusal::Forbidden)
        }
        (Standing::Owner, Grant | Revoke) => Step::ReadTarget(Authority::Owner),
        (Standing::Admin, Grant | Revoke) => Step::ReadTarget(Authority::Admin),
        (Standing::Owner, Leave) => Step::Refuse(Refusal::OwnerCannotBeChanged),
    }
}

/// Decides a grant or a revoke under `authority` once its target is known.
pub(crate) fn decide_target(
    authority: Authority,
    action: TargetAction,
    target: Target,
) -> Decision {
    use Role::{Admin, Reader};
    let refuse = Decision::Refuse;
    match (authority, action, target) {
        (_, _, Target::Owner) => refuse(Refusal::OwnerCannotBeChanged),
        (_, TargetAction::Revoke, Target::Person(None)) => refuse(Refusal::GrantNotFound),
        (Authority::Owner, _, Target::Person(_))
        | (Authority::Admin, TargetAction::Grant(Reader), Target::Person(None | Some(Reader)))
        | (Authority::Admin, TargetAction::Revoke, Target::Person(Some(Reader))) => Decision::Allow,
        (Authority::Admin, TargetAction::Grant(Admin), Target::Person(_))
        | (Authority::Admin, TargetAction::Grant(Reader), Target::Person(Some(Admin)))
        | (Authority::Admin, TargetAction::Revoke, Target::Person(Some(Admin))) => {
            refuse(Refusal::Forbidden)
        }
    }
}

#[cfg(test)]
mod tests;

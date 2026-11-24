//! Grants: the people a share's owner, or an admin of it, opens it to.
//!
//! The owner of a share is a field of the share and has no grant. Every
//! other person who may open it holds one grant with a [`Role`].

use crate::name::PrincipalId;
use crate::share::{HiddenPrincipal, ShareId};
use crate::time::Timestamp;
use std::fmt;

/// The most grants one share has. A grant to a new person on a share that
/// has this many answers [`GrantStoreError::TooMany`](crate::GrantStoreError)
/// and writes nothing; a role change of a person who holds a grant still
/// applies, and a revoke frees a place.
pub const MAX_GRANTS: usize = 256;

/// What a grant lets its holder do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// Reads every version of the share and its files, and may leave.
    Reader,
    /// Also lists the share's grants, grants and revokes readers, and may
    /// leave.
    Admin,
}

/// One person's grant on one share.
#[derive(Clone, PartialEq, Eq)]
pub struct Grant {
    share: ShareId,
    grantee: PrincipalId,
    role: Role,
    granted_by: PrincipalId,
    created: Timestamp,
}

impl Grant {
    /// Builds a grant as a store read it back.
    #[must_use]
    pub fn new(
        share: ShareId,
        grantee: PrincipalId,
        role: Role,
        granted_by: PrincipalId,
        created: Timestamp,
    ) -> Self {
        Self {
            share,
            grantee,
            role,
            granted_by,
            created,
        }
    }

    /// The share.
    #[must_use]
    pub fn share(&self) -> &ShareId {
        &self.share
    }

    /// The person who holds the grant.
    #[must_use]
    pub fn grantee(&self) -> &PrincipalId {
        &self.grantee
    }

    /// The role.
    #[must_use]
    pub fn role(&self) -> Role {
        self.role
    }

    /// The person who wrote the grant's current role.
    #[must_use]
    pub fn granted_by(&self) -> &PrincipalId {
        &self.granted_by
    }

    /// When the person was first granted the share. A role change keeps it.
    #[must_use]
    pub fn created(&self) -> Timestamp {
        self.created
    }
}

impl fmt::Debug for Grant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Grant")
            .field("share", &self.share)
            .field("grantee", &HiddenPrincipal)
            .field("role", &self.role)
            .field("granted_by", &HiddenPrincipal)
            .field("created", &self.created)
            .finish()
    }
}

/// The authority a grant acts under.
#[derive(Clone, PartialEq, Eq)]
pub enum Granter {
    /// The share's owner.
    Owner(PrincipalId),
    /// A person who holds an Admin grant on the share. A store applies the
    /// write only while that person still holds it.
    Admin(PrincipalId),
}

impl Granter {
    /// The person the write acts for.
    #[must_use]
    pub fn principal(&self) -> &PrincipalId {
        match self {
            Self::Owner(principal) | Self::Admin(principal) => principal,
        }
    }

    /// The admin whose Admin grant a store checks at write time, if any.
    #[must_use]
    pub fn admin(&self) -> Option<&PrincipalId> {
        match self {
            Self::Owner(_) => None,
            Self::Admin(principal) => Some(principal),
        }
    }
}

impl fmt::Debug for Granter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Owner(_) => formatter
                .debug_tuple("Owner")
                .field(&HiddenPrincipal)
                .finish(),
            Self::Admin(_) => formatter
                .debug_tuple("Admin")
                .field(&HiddenPrincipal)
                .finish(),
        }
    }
}

/// The authority a revoke acts under.
#[derive(Clone, PartialEq, Eq)]
pub enum Revoker {
    /// The share's owner.
    Owner(PrincipalId),
    /// A person who holds an Admin grant on the share. A store applies the
    /// revoke only while that person still holds it.
    Admin(PrincipalId),
    /// The grantee, ending their own grant. A store applies it under the
    /// expected role alone.
    Grantee,
}

impl Revoker {
    /// The admin whose Admin grant a store checks at write time, if any.
    #[must_use]
    pub fn admin(&self) -> Option<&PrincipalId> {
        match self {
            Self::Owner(_) | Self::Grantee => None,
            Self::Admin(principal) => Some(principal),
        }
    }
}

impl fmt::Debug for Revoker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Owner(_) => formatter
                .debug_tuple("Owner")
                .field(&HiddenPrincipal)
                .finish(),
            Self::Admin(_) => formatter
                .debug_tuple("Admin")
                .field(&HiddenPrincipal)
                .finish(),
            Self::Grantee => formatter.write_str("Grantee"),
        }
    }
}

/// A grant to write: give `grantee` the role `role` on `share`, if the
/// grantee's role is still `expected`. Only this crate builds one.
#[derive(Clone, PartialEq, Eq)]
pub struct GrantWrite {
    share: ShareId,
    grantee: PrincipalId,
    role: Role,
    expected: Option<Role>,
    by: Granter,
    created: Timestamp,
}

impl GrantWrite {
    pub(crate) fn new(
        share: ShareId,
        grantee: PrincipalId,
        role: Role,
        expected: Option<Role>,
        by: Granter,
        created: Timestamp,
    ) -> Self {
        Self {
            share,
            grantee,
            role,
            expected,
            by,
            created,
        }
    }

    /// The share.
    #[must_use]
    pub fn share(&self) -> &ShareId {
        &self.share
    }

    /// The person the grant is for.
    #[must_use]
    pub fn grantee(&self) -> &PrincipalId {
        &self.grantee
    }

    /// The role to give.
    #[must_use]
    pub fn role(&self) -> Role {
        self.role
    }

    /// The role the writer saw, or `None` when the grantee held no grant.
    #[must_use]
    pub fn expected(&self) -> Option<Role> {
        self.expected
    }

    /// The authority the write acts under.
    #[must_use]
    pub fn by(&self) -> &Granter {
        &self.by
    }

    /// When the grant is written. A store keeps it only for a new grant.
    #[must_use]
    pub fn created(&self) -> Timestamp {
        self.created
    }

    /// The grant a store holds after it applies this write over `kept`,
    /// the grant it replaces, whose time of first grant it keeps.
    #[must_use]
    pub fn applied(&self, kept: Option<&Grant>) -> Grant {
        Grant::new(
            self.share.clone(),
            self.grantee.clone(),
            self.role,
            self.by.principal().clone(),
            kept.map_or(self.created, Grant::created),
        )
    }
}

impl fmt::Debug for GrantWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GrantWrite")
            .field("share", &self.share)
            .field("grantee", &HiddenPrincipal)
            .field("role", &self.role)
            .field("expected", &self.expected)
            .field("by", &self.by)
            .field("created", &self.created)
            .finish()
    }
}

/// A grant to end: remove `grantee`'s grant on `share`, if its role is still
/// `expected`. Only this crate builds one.
#[derive(Clone, PartialEq, Eq)]
pub struct GrantRevoke {
    share: ShareId,
    grantee: PrincipalId,
    expected: Role,
    by: Revoker,
}

impl GrantRevoke {
    pub(crate) fn new(share: ShareId, grantee: PrincipalId, expected: Role, by: Revoker) -> Self {
        Self {
            share,
            grantee,
            expected,
            by,
        }
    }

    /// The share.
    #[must_use]
    pub fn share(&self) -> &ShareId {
        &self.share
    }

    /// The person whose grant ends.
    #[must_use]
    pub fn grantee(&self) -> &PrincipalId {
        &self.grantee
    }

    /// The role the writer saw.
    #[must_use]
    pub fn expected(&self) -> Role {
        self.expected
    }

    /// The authority the revoke acts under.
    #[must_use]
    pub fn by(&self) -> &Revoker {
        &self.by
    }
}

impl fmt::Debug for GrantRevoke {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GrantRevoke")
            .field("share", &self.share)
            .field("grantee", &HiddenPrincipal)
            .field("expected", &self.expected)
            .field("by", &self.by)
            .finish()
    }
}

#[cfg(test)]
mod tests;

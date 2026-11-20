//! [`DynamoGrantStore`], hangar-core's `GrantStore` on the table.
//!
//! A grant or a revoke first reads the grant item. When its role is not the
//! one the writer expected, the store answers without writing, after it
//! checks an acting admin's authority, because stale authority comes before
//! a changed grant. Otherwise one transaction checks the admin's authority,
//! writes the grant item only at the `rev` that was read, writes the
//! grantee's entry and keeps the share's grant count.

use crate::dynamodb::Table;
use crate::dynamodb::cancel::Classified;
use crate::dynamodb::items::{Held, ItemKind, Rev, grant_from, grantee_entry_from, role_of};
use crate::dynamodb::keys::{ENTRY_PREFIX, GRANT_PREFIX, Key, grantee_partition, share_partition};
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::dynamodb::settle::{self, malformed};
use crate::dynamodb::transact::{self, GrantPlan, ItemRole, RevokePlan, Transaction};
use hangar_core::{
    Grant, GrantRevoke, GrantStore, GrantStoreError, GrantWrite, Granter, Listing, MAX_GRANTS,
    MAX_LISTED, PrincipalId, Revoker, Role, ShareId, Timestamp,
};

/// Keeps the grants of shares in the table.
///
/// A grant item, the grantee's entry and the share's grant count change in
/// one transaction. Every read is consistent, so a revoke is visible to the
/// next read of the grant and of both lists.
#[derive(Debug, Clone)]
pub struct DynamoGrantStore {
    table: Table,
}

impl DynamoGrantStore {
    pub(crate) fn from_table(table: Table) -> Self {
        Self { table }
    }
}

/// Reads `grantee`'s grant on `share`, with its `rev`.
async fn read_grant(
    table: &Table,
    share: &ShareId,
    grantee: &PrincipalId,
) -> Result<Option<Held>, GrantStoreError> {
    let item = table
        .get(&Key::grant(share, grantee), ItemKind::Grant)
        .await
        .map_err(|_| GrantStoreError::Unavailable)?;
    item.map(|item| grant_from(&item, share))
        .transpose()
        .map_err(|error| {
            malformed(error);
            GrantStoreError::Unavailable
        })
}

/// The answer to a write whose expected role differs from `current`: stale
/// authority when the acting admin no longer holds the Admin grant, and
/// otherwise the grant's current role.
async fn mismatch<T>(
    table: &Table,
    share: &ShareId,
    grantee: &PrincipalId,
    admin: Option<&PrincipalId>,
    current: Option<Role>,
) -> Sent<T, GrantStoreError> {
    let Some(admin) = admin else {
        return Sent::Refused(GrantStoreError::Changed { current });
    };
    let admin_role = if admin == grantee {
        current
    } else {
        match read_grant(table, share, admin).await {
            Ok(held) => held.map(|held| held.grant.role()),
            Err(_) => return Sent::Unavailable,
        }
    };
    if admin_role == Some(Role::Admin) {
        Sent::Refused(GrantStoreError::Changed { current })
    } else {
        Sent::Refused(GrantStoreError::StaleAuthority)
    }
}

/// What a classified failure of a grant or a revoke means. `folded` says
/// whether the admin's check joined the grant item's condition, because the
/// admin acts on their own grant.
fn refusal<T>(
    classified: Classified,
    expected: Option<Role>,
    folded: bool,
) -> Sent<T, GrantStoreError> {
    match classified {
        Classified::Conflict {
            role: ItemRole::AdminCheck,
            ..
        } => Sent::Refused(GrantStoreError::StaleAuthority),
        Classified::Conflict {
            role: ItemRole::GrantItem,
            old,
        } => {
            let current = old.as_ref().and_then(role_of);
            if old.is_some() && current.is_none() {
                Sent::Unavailable
            } else if folded && current != Some(Role::Admin) {
                Sent::Refused(GrantStoreError::StaleAuthority)
            } else if current != expected {
                Sent::Refused(GrantStoreError::Changed { current })
            } else {
                // The same role with another `rev`: a revoke and a re-grant
                // ran between the read and the write, so read again.
                Sent::Again
            }
        }
        Classified::Conflict {
            role: ItemRole::GrantCount,
            ..
        } => Sent::Refused(GrantStoreError::TooMany),
        Classified::Conflict { .. } | Classified::Unavailable(_) => Sent::Unavailable,
        Classified::Retry => Sent::Again,
        Classified::Unknown => Sent::Unknown,
    }
}

fn answer<T>(answer: Final<T, GrantStoreError>) -> Result<T, GrantStoreError> {
    match answer {
        Final::Done(done) => Ok(done),
        Final::Refused(refused) => Err(refused),
        Final::Unavailable => Err(GrantStoreError::Unavailable),
    }
}

/// A grant, read from a `GrantWrite`.
#[derive(Debug)]
pub(crate) struct GrantOp<'a> {
    pub(crate) table: &'a Table,
    pub(crate) share: ShareId,
    pub(crate) grantee: PrincipalId,
    pub(crate) role: Role,
    pub(crate) expected: Option<Role>,
    pub(crate) by: Granter,
    pub(crate) created: Timestamp,
}

/// A built grant: the transaction, the grant it writes and its new `rev`.
pub(crate) struct BuiltGrant {
    transaction: Transaction,
    grant: Grant,
    rev: Rev,
}

impl GrantOp<'_> {
    fn folded(&self) -> bool {
        self.by.admin() == Some(&self.grantee)
    }
}

impl Attempt for GrantOp<'_> {
    type Built = BuiltGrant;
    type Done = Grant;
    type Refused = GrantStoreError;

    async fn build(&self) -> Result<BuiltGrant, Sent<Grant, GrantStoreError>> {
        let held = read_grant(self.table, &self.share, &self.grantee)
            .await
            .map_err(|_| Sent::Unavailable)?;
        let current = held.as_ref().map(|held| held.grant.role());
        if current != self.expected || (self.folded() && current != Some(Role::Admin)) {
            let admin = self.by.admin();
            return Err(mismatch(self.table, &self.share, &self.grantee, admin, current).await);
        }
        let rev = Rev::fresh().ok_or(Sent::Unavailable)?;
        let created = held
            .as_ref()
            .map_or(self.created, |held| held.grant.created());
        let plan = GrantPlan {
            share: &self.share,
            grantee: &self.grantee,
            role: self.role,
            by: &self.by,
            created,
            read: held.as_ref().map(|held| &held.rev),
            rev: &rev,
        };
        let transaction = transact::grant(self.table.name(), &plan).ok_or(Sent::Unavailable)?;
        let grant = Grant::new(
            self.share.clone(),
            self.grantee.clone(),
            self.role,
            self.by.principal().clone(),
            created,
        );
        Ok(BuiltGrant {
            transaction,
            grant,
            rev,
        })
    }

    async fn send(&self, built: &BuiltGrant, token: &Token) -> Sent<Grant, GrantStoreError> {
        match self
            .table
            .transact(&built.transaction, token, "grant")
            .await
        {
            Ok(()) => Sent::Done(built.grant.clone()),
            Err(classified) => refusal(classified, self.expected, self.folded()),
        }
    }

    async fn settle(&self, built: &BuiltGrant) -> Settled<Grant> {
        settle::granted(self.table, &built.grant, &built.rev).await
    }
}

/// A revoke, read from a `GrantRevoke`.
#[derive(Debug)]
pub(crate) struct RevokeOp<'a> {
    pub(crate) table: &'a Table,
    pub(crate) share: ShareId,
    pub(crate) grantee: PrincipalId,
    pub(crate) expected: Role,
    pub(crate) by: Revoker,
}

impl RevokeOp<'_> {
    fn folded(&self) -> bool {
        self.by.admin() == Some(&self.grantee)
    }
}

impl Attempt for RevokeOp<'_> {
    type Built = Transaction;
    type Done = ();
    type Refused = GrantStoreError;

    async fn build(&self) -> Result<Transaction, Sent<(), GrantStoreError>> {
        let held = read_grant(self.table, &self.share, &self.grantee)
            .await
            .map_err(|_| Sent::Unavailable)?;
        let current = held.as_ref().map(|held| held.grant.role());
        let folded_stale = self.folded() && current != Some(Role::Admin);
        let Some(held) = held.filter(|_| current == Some(self.expected) && !folded_stale) else {
            let admin = self.by.admin();
            return Err(mismatch(self.table, &self.share, &self.grantee, admin, current).await);
        };
        let plan = RevokePlan {
            share: &self.share,
            grantee: &self.grantee,
            by: &self.by,
            created: held.grant.created(),
            read: &held.rev,
        };
        transact::revoke(self.table.name(), &plan).ok_or(Sent::Unavailable)
    }

    async fn send(&self, built: &Transaction, token: &Token) -> Sent<(), GrantStoreError> {
        match self.table.transact(built, token, "revoke").await {
            Ok(()) => Sent::Done(()),
            Err(classified) => refusal(classified, Some(self.expected), self.folded()),
        }
    }

    async fn settle(&self, _built: &Transaction) -> Settled<()> {
        settle::revoked(self.table, &self.share, &self.grantee).await
    }
}

impl GrantStore for DynamoGrantStore {
    async fn grant(&self, write: GrantWrite) -> Result<Grant, GrantStoreError> {
        let op = GrantOp {
            table: &self.table,
            share: write.share().clone(),
            grantee: write.grantee().clone(),
            role: write.role(),
            expected: write.expected(),
            by: write.by().clone(),
            created: write.created(),
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn revoke(&self, revoke: GrantRevoke) -> Result<(), GrantStoreError> {
        let op = RevokeOp {
            table: &self.table,
            share: revoke.share().clone(),
            grantee: revoke.grantee().clone(),
            expected: revoke.expected(),
            by: revoke.by().clone(),
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn get(
        &self,
        share: &ShareId,
        grantee: &PrincipalId,
    ) -> Result<Option<Grant>, GrantStoreError> {
        Ok(read_grant(&self.table, share, grantee)
            .await?
            .map(|held| held.grant))
    }

    async fn of_share(&self, share: &ShareId) -> Result<Vec<Grant>, GrantStoreError> {
        // One item more than a share may hold shows a count gone wrong,
        // which the store logs.
        let items = self
            .table
            .query(
                &share_partition(share),
                GRANT_PREFIX,
                MAX_GRANTS + 1,
                ItemKind::Grant,
            )
            .await
            .map_err(|_| GrantStoreError::Unavailable)?;
        let mut grants = items
            .iter()
            .map(|item| grant_from(item, share).map(|held| held.grant))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                malformed(error);
                GrantStoreError::Unavailable
            })?;
        grants.sort_by(|a, b| {
            a.created()
                .cmp(&b.created())
                .then_with(|| a.grantee().cmp(b.grantee()))
        });
        if grants.len() > MAX_GRANTS {
            tracing::warn!(item = "grant", "a share holds more grants than its limit");
            grants.truncate(MAX_GRANTS);
        }
        Ok(grants)
    }

    async fn granted_to(&self, grantee: &PrincipalId) -> Result<Listing<Grant>, GrantStoreError> {
        // One entry more than a listing holds tells whether more exist.
        let items = self
            .table
            .query(
                &grantee_partition(grantee),
                ENTRY_PREFIX,
                MAX_LISTED + 1,
                ItemKind::GranteeEntry,
            )
            .await
            .map_err(|_| GrantStoreError::Unavailable)?;
        let grants = items
            .iter()
            .map(|item| grantee_entry_from(item, grantee))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                malformed(error);
                GrantStoreError::Unavailable
            })?;
        Ok(Listing::new(grants))
    }
}

#[cfg(test)]
mod tests;

//! [`DynamoShareStore`], flare-core's `ShareStore` on the table.

use crate::dynamodb::Table;
use crate::dynamodb::cancel::Classified;
use crate::dynamodb::items::{ItemKind, owner_entry_from, share_from, version_from};
use crate::dynamodb::keys::{ENTRY_PREFIX, Key, owner_partition};
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::dynamodb::settle::{self, malformed};
use crate::dynamodb::transact::{self, ItemRole, Transaction};
use flare_core::{
    Listing, MAX_LISTED, MAX_VERSIONS, NewShare, NewVersion, PrincipalId, Share, ShareId,
    ShareStore, ShareStoreError, ShareVersion, SharedVersion,
};

/// Keeps shares and their versions in the table.
///
/// A share's item, each of its versions and its entry in the owner's list
/// are separate items that one transaction writes together. Every read is
/// consistent, so it sees every write that finished before it.
#[derive(Debug, Clone)]
pub struct DynamoShareStore {
    table: Table,
}

impl DynamoShareStore {
    pub(crate) fn from_table(table: Table) -> Self {
        Self { table }
    }

    /// The share `id` as the table holds it, or `None`.
    async fn read(&self, id: &ShareId) -> Result<Option<Share>, ShareStoreError> {
        read_share(&self.table, id).await
    }
}

async fn read_share(table: &Table, id: &ShareId) -> Result<Option<Share>, ShareStoreError> {
    let item = table
        .get(&Key::share(id), ItemKind::Share)
        .await
        .map_err(|_| ShareStoreError::Unavailable)?;
    item.map(|item| share_from(&item, id))
        .transpose()
        .map_err(|error| {
            malformed(error);
            ShareStoreError::Unavailable
        })
}

/// Turns the loop's answer into the store's.
fn answer<T>(answer: Final<T, ShareStoreError>) -> Result<T, ShareStoreError> {
    match answer {
        Final::Done(done) => Ok(done),
        Final::Refused(refused) => Err(refused),
        Final::Unavailable => Err(ShareStoreError::Unavailable),
    }
}

/// What a classified failure of a share write means, given what each failed
/// item's role means to this write.
fn refusal<T>(
    classified: Classified,
    conflict: impl FnOnce(ItemRole) -> Sent<T, ShareStoreError>,
) -> Sent<T, ShareStoreError> {
    match classified {
        Classified::Conflict { role, .. } => conflict(role),
        Classified::Retry => Sent::Again,
        Classified::Unknown => Sent::Unknown,
        Classified::Unavailable(_) => Sent::Unavailable,
    }
}

/// A create of one share with its version 1.
struct CreateOp<'a> {
    table: &'a Table,
    share: Share,
    first: SharedVersion,
}

impl Attempt for CreateOp<'_> {
    type Built = Transaction;
    type Done = Share;
    type Refused = ShareStoreError;

    async fn build(&self) -> Result<Transaction, Sent<Share, ShareStoreError>> {
        transact::create(self.table.name(), &self.share, &self.first).ok_or(Sent::Unavailable)
    }

    async fn send(&self, built: &Transaction, token: &Token) -> Sent<Share, ShareStoreError> {
        match self.table.transact(built, token, "create").await {
            Ok(()) => Sent::Done(self.share.clone()),
            Err(classified) => refusal(classified, |role| match role {
                ItemRole::ShareItem => Sent::Refused(ShareStoreError::IdTaken),
                ItemRole::ShareCount => Sent::Refused(ShareStoreError::TooManyShares),
                _ => Sent::Unavailable,
            }),
        }
    }

    async fn settle(&self, _built: &Transaction) -> Settled<Share> {
        settle::created(self.table, &self.share).await
    }
}

/// A publish of one version, built from a fresh read of the share.
struct PublishOp<'a> {
    table: &'a Table,
    id: &'a ShareId,
    version: &'a NewVersion,
}

/// A built publish: the transaction and the version it writes.
struct BuiltPublish {
    transaction: Transaction,
    version: SharedVersion,
}

impl Attempt for PublishOp<'_> {
    type Built = BuiltPublish;
    type Done = ShareVersion;
    type Refused = ShareStoreError;

    async fn build(&self) -> Result<BuiltPublish, Sent<ShareVersion, ShareStoreError>> {
        let share = match read_share(self.table, self.id).await {
            Ok(Some(share)) => share,
            Ok(None) => return Err(Sent::Refused(ShareStoreError::NotFound)),
            Err(_) => return Err(Sent::Unavailable),
        };
        if share.latest().get() >= MAX_VERSIONS {
            return Err(Sent::Refused(ShareStoreError::TooManyVersions));
        }
        let next = share.latest().next().ok_or(Sent::Unavailable)?;
        let version = self.version.shared(self.id.clone(), next);
        let transaction =
            transact::publish(self.table.name(), &share, &version).ok_or(Sent::Unavailable)?;
        Ok(BuiltPublish {
            transaction,
            version,
        })
    }

    async fn send(
        &self,
        built: &BuiltPublish,
        token: &Token,
    ) -> Sent<ShareVersion, ShareStoreError> {
        match self
            .table
            .transact(&built.transaction, token, "publish")
            .await
        {
            Ok(()) => Sent::Done(built.version.number()),
            // Another publish moved `latest` since the read, or wrote the
            // number first; a fresh read takes the next number.
            Err(classified) => refusal(classified, |role| match role {
                ItemRole::ShareItem | ItemRole::Version => Sent::Again,
                _ => Sent::Unavailable,
            }),
        }
    }

    async fn settle(&self, built: &BuiltPublish) -> Settled<ShareVersion> {
        settle::published(self.table, &built.version).await
    }
}

impl ShareStore for DynamoShareStore {
    async fn create(&self, new: NewShare) -> Result<Share, ShareStoreError> {
        let op = CreateOp {
            table: &self.table,
            share: new.share(),
            first: new.first_version(),
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn get(&self, id: &ShareId) -> Result<Share, ShareStoreError> {
        self.read(id).await?.ok_or(ShareStoreError::NotFound)
    }

    async fn publish(
        &self,
        id: &ShareId,
        version: NewVersion,
    ) -> Result<ShareVersion, ShareStoreError> {
        let op = PublishOp {
            table: &self.table,
            id,
            version: &version,
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn version(
        &self,
        id: &ShareId,
        number: ShareVersion,
    ) -> Result<SharedVersion, ShareStoreError> {
        let item = self
            .table
            .get(&Key::version(id, number), ItemKind::Version)
            .await
            .map_err(|_| ShareStoreError::Unavailable)?
            .ok_or(ShareStoreError::NotFound)?;
        version_from(&item, id, number).map_err(|error| {
            malformed(error);
            ShareStoreError::Unavailable
        })
    }

    async fn owned_by(&self, owner: &PrincipalId) -> Result<Listing<Share>, ShareStoreError> {
        // One entry more than a listing holds tells whether more exist.
        let items = self
            .table
            .query(
                &owner_partition(owner),
                ENTRY_PREFIX,
                MAX_LISTED + 1,
                ItemKind::OwnerEntry,
            )
            .await
            .map_err(|_| ShareStoreError::Unavailable)?;
        let shares = items
            .iter()
            .map(|item| owner_entry_from(item, owner))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                malformed(error);
                ShareStoreError::Unavailable
            })?;
        Ok(Listing::new(shares))
    }
}

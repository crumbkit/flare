//! [`DynamoTokenStore`], flare-core's `TokenStore` on the table.
//!
//! Each write is one transaction that the retry loop sends, and a write
//! whose answer is lost is settled by a consistent read of the item it
//! wrote: an issue by the code's owner, port and expiry, a use by its
//! `used_by` mark, a create by the token's digest, and a revoke by the
//! token's revoked time.

mod items;

use crate::dynamodb::Table;
use crate::dynamodb::cancel::Classified;
use crate::dynamodb::items::ItemKind;
use crate::dynamodb::keys::Key;
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::dynamodb::settle::malformed;
use crate::dynamodb::transact::{ItemRole, Transaction};
use flare_core::{
    Digest, Email, LoginCode, NewToken, StoredToken, Timestamp, TokenId, TokenStore,
    TokenStoreError, UsedCode,
};
use items::{HeldCode, SlotRead, code_from, slot_token, token_from};

/// Keeps login codes and CLI tokens in the table, by digest.
///
/// A login code expires 60 seconds after it is issued and the table's TTL
/// deletes it later; until then a read treats it as expired by its time. A
/// token stays after it expires or is revoked, so a check of it answers
/// that it no longer signs in.
#[derive(Debug, Clone)]
pub struct DynamoTokenStore {
    table: Table,
}

impl DynamoTokenStore {
    pub(crate) fn from_table(table: Table) -> Self {
        Self { table }
    }
}

/// Turns the loop's answer into the store's.
fn answer<T>(answer: Final<T, TokenStoreError>) -> Result<T, TokenStoreError> {
    match answer {
        Final::Done(done) => Ok(done),
        Final::Refused(refused) => Err(refused),
        Final::Unavailable => Err(TokenStoreError::Unavailable),
    }
}

/// What a failed token write means, given what the failed item's role
/// means to this write.
fn refusal<T>(
    classified: Classified,
    conflict: impl FnOnce(ItemRole) -> Sent<T, TokenStoreError>,
) -> Sent<T, TokenStoreError> {
    match classified {
        Classified::Conflict { role, .. } => conflict(role),
        Classified::Retry => Sent::Again,
        Classified::Unknown => Sent::Unknown,
        Classified::Unavailable(_) => Sent::Unavailable,
    }
}

async fn read_code(table: &Table, digest: &Digest) -> Result<Option<HeldCode>, ()> {
    let item = table
        .get(&Key::code(digest), ItemKind::Code)
        .await
        .map_err(|_| ())?;
    item.map(|item| code_from(&item))
        .transpose()
        .map_err(malformed)
}

async fn read_token(table: &Table, id: &TokenId) -> Result<Option<StoredToken>, ()> {
    let item = table
        .get(&Key::token(id), ItemKind::Token)
        .await
        .map_err(|_| ())?;
    item.map(|item| token_from(&item, id))
        .transpose()
        .map_err(malformed)
}

/// An issue of one login code.
struct IssueOp<'a> {
    table: &'a Table,
    code: &'a LoginCode,
}

impl Attempt for IssueOp<'_> {
    type Built = Transaction;
    type Done = ();
    type Refused = TokenStoreError;

    async fn build(&self) -> Result<Transaction, Sent<(), TokenStoreError>> {
        items::issue(self.table.name(), self.code).ok_or(Sent::Unavailable)
    }

    async fn send(&self, built: &Transaction, token: &Token) -> Sent<(), TokenStoreError> {
        match self.table.transact(built, token, "issue code").await {
            Ok(()) => Sent::Done(()),
            Err(classified) => refusal(classified, |role| match role {
                ItemRole::CodeItem => Sent::Refused(TokenStoreError::Taken),
                _ => Sent::Unavailable,
            }),
        }
    }

    async fn settle(&self, _built: &Transaction) -> Settled<()> {
        match read_code(self.table, self.code.digest()).await {
            Ok(Some(held))
                if &held.owner == self.code.owner()
                    && held.port == self.code.port()
                    && held.expires == self.code.expires() =>
            {
                Settled::Matches(())
            }
            Ok(_) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

/// A use of one login code, marked with a random value of its own.
struct UseOp<'a> {
    table: &'a Table,
    digest: &'a Digest,
    now: Timestamp,
}

/// A built use: the transaction, its mark and the code it spends.
struct BuiltUse {
    transaction: Transaction,
    mark: String,
    code: HeldCode,
}

impl Attempt for UseOp<'_> {
    type Built = BuiltUse;
    type Done = UsedCode;
    type Refused = TokenStoreError;

    async fn build(&self) -> Result<BuiltUse, Sent<UsedCode, TokenStoreError>> {
        let code = match read_code(self.table, self.digest).await {
            Ok(Some(code)) if code.usable(self.now) => code,
            Ok(_) => return Err(Sent::Refused(TokenStoreError::CodeRefused)),
            Err(()) => return Err(Sent::Unavailable),
        };
        let mark = crate::dynamodb::random_hex().ok_or(Sent::Unavailable)?;
        let transaction = items::spend(self.table.name(), self.digest, &mark, self.now)
            .ok_or(Sent::Unavailable)?;
        Ok(BuiltUse {
            transaction,
            mark,
            code,
        })
    }

    async fn send(&self, built: &BuiltUse, token: &Token) -> Sent<UsedCode, TokenStoreError> {
        match self
            .table
            .transact(&built.transaction, token, "use code")
            .await
        {
            Ok(()) => Sent::Done(built.code.used()),
            Err(classified) => refusal(classified, |role| match role {
                ItemRole::CodeItem => Sent::Refused(TokenStoreError::CodeRefused),
                _ => Sent::Unavailable,
            }),
        }
    }

    async fn settle(&self, built: &BuiltUse) -> Settled<UsedCode> {
        match read_code(self.table, self.digest).await {
            Ok(Some(held)) if held.used_by.as_deref() == Some(built.mark.as_str()) => {
                Settled::Matches(held.used())
            }
            Ok(_) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

/// A create of one token, which revokes the owner's active token with the
/// same label.
struct CreateOp<'a> {
    table: &'a Table,
    token: &'a NewToken,
}

/// A built create: the transaction and the token it revokes.
struct BuiltCreate {
    transaction: Transaction,
    previous: Option<TokenId>,
}

impl Attempt for CreateOp<'_> {
    type Built = BuiltCreate;
    type Done = Option<TokenId>;
    type Refused = TokenStoreError;

    async fn build(&self) -> Result<BuiltCreate, Sent<Option<TokenId>, TokenStoreError>> {
        let key = Key::label_slot(self.token.owner(), self.token.label());
        let slot = match self.table.get(&key, ItemKind::LabelSlot).await {
            Ok(None) => SlotRead::Absent,
            Ok(Some(item)) => match slot_token(&item) {
                Ok(id) => SlotRead::Names(id),
                Err(error) => {
                    malformed(error);
                    return Err(Sent::Unavailable);
                }
            },
            Err(_) => return Err(Sent::Unavailable),
        };
        let previous = match &slot {
            SlotRead::Absent => None,
            SlotRead::Names(id) => match read_token(self.table, id).await {
                Ok(Some(old)) if old.revoked().is_none() => Some(id.clone()),
                Ok(_) => None,
                Err(()) => return Err(Sent::Unavailable),
            },
        };
        let transaction = items::create(self.table.name(), self.token, &slot, previous.as_ref())
            .ok_or(Sent::Unavailable)?;
        Ok(BuiltCreate {
            transaction,
            previous,
        })
    }

    async fn send(
        &self,
        built: &BuiltCreate,
        token: &Token,
    ) -> Sent<Option<TokenId>, TokenStoreError> {
        match self
            .table
            .transact(&built.transaction, token, "create token")
            .await
        {
            Ok(()) => Sent::Done(built.previous.clone()),
            // A slot or an old token that changed since the read is read
            // again.
            Err(classified) => refusal(classified, |role| match role {
                ItemRole::TokenItem => Sent::Refused(TokenStoreError::Taken),
                ItemRole::LabelSlot | ItemRole::OldToken => Sent::Again,
                _ => Sent::Unavailable,
            }),
        }
    }

    async fn settle(&self, built: &BuiltCreate) -> Settled<Option<TokenId>> {
        match read_token(self.table, self.token.id()).await {
            Ok(Some(held)) if held.digest() == self.token.clone().stored().digest() => {
                Settled::Matches(built.previous.clone())
            }
            Ok(_) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

/// A revoke of one token by its owner.
struct RevokeOp<'a> {
    table: &'a Table,
    id: &'a TokenId,
    owner: &'a Email,
    at: Timestamp,
}

impl Attempt for RevokeOp<'_> {
    type Built = Transaction;
    type Done = ();
    type Refused = TokenStoreError;

    async fn build(&self) -> Result<Transaction, Sent<(), TokenStoreError>> {
        match read_token(self.table, self.id).await {
            Ok(Some(held)) if held.owner() == self.owner => {
                if held.revoked().is_some() {
                    return Err(Sent::Done(()));
                }
            }
            Ok(_) => return Err(Sent::Refused(TokenStoreError::NotFound)),
            Err(()) => return Err(Sent::Unavailable),
        }
        items::revoke(self.table.name(), self.id, self.owner, self.at).ok_or(Sent::Unavailable)
    }

    async fn send(&self, built: &Transaction, token: &Token) -> Sent<(), TokenStoreError> {
        match self.table.transact(built, token, "revoke token").await {
            Ok(()) => Sent::Done(()),
            // A revoke that ran since the read leaves its own time, which
            // the next read finds.
            Err(classified) => refusal(classified, |role| match role {
                ItemRole::TokenItem => Sent::Again,
                _ => Sent::Unavailable,
            }),
        }
    }

    async fn settle(&self, _built: &Transaction) -> Settled<()> {
        match read_token(self.table, self.id).await {
            Ok(Some(held)) if held.revoked().is_some() => Settled::Matches(()),
            Ok(_) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

impl TokenStore for DynamoTokenStore {
    async fn issue_code(&self, code: LoginCode) -> Result<(), TokenStoreError> {
        let op = IssueOp {
            table: &self.table,
            code: &code,
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn use_code(&self, digest: &Digest, now: Timestamp) -> Result<UsedCode, TokenStoreError> {
        let op = UseOp {
            table: &self.table,
            digest,
            now,
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn create(&self, token: NewToken) -> Result<Option<TokenId>, TokenStoreError> {
        let op = CreateOp {
            table: &self.table,
            token: &token,
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn get(&self, id: &TokenId) -> Result<StoredToken, TokenStoreError> {
        read_token(&self.table, id)
            .await
            .map_err(|()| TokenStoreError::Unavailable)?
            .ok_or(TokenStoreError::NotFound)
    }

    async fn revoke(
        &self,
        id: &TokenId,
        owner: &Email,
        at: Timestamp,
    ) -> Result<(), TokenStoreError> {
        let op = RevokeOp {
            table: &self.table,
            id,
            owner,
            at,
        };
        answer(run(self.table.policy(), &op).await)
    }
}

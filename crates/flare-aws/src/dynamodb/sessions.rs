//! [`DynamoSessionStore`], flare-core's `SessionStore` on the table.
//!
//! A sign-in, a removal and an end of all of a person's sessions are
//! transactions that the retry loop sends. A request that extends a session
//! sends one transaction. A failure that leaves the session as it was is
//! ignored, because the session is still live under its old expiry and the
//! next request tries again; a failed condition reads the session again,
//! and a session that is gone answers as unknown. Every
//! read is consistent, and every answer compares the session's expiry with
//! `now` first, so a session that the table's TTL has not deleted yet is
//! still expired.

mod end_all;
mod items;

use crate::dynamodb::Table;
use crate::dynamodb::cancel::Classified;
use crate::dynamodb::items::{ItemKind, Rev};
use crate::dynamodb::keys::{Key, live_links, session_partition};
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::dynamodb::settle::malformed;
use crate::dynamodb::transact::{ItemRole, Transaction};
use end_all::RemoveAllOp;
use flare_core::{
    Digest, MAX_SESSIONS_PER_PERSON, NewSession, Principal, PrincipalId, SessionRule, SessionStore,
    SessionStoreError, Timestamp,
};
use items::{Creation, Ended, GuardRead, HeldSession, guard_rev, links_of, session_from};

/// How many links one read of a person's sessions takes. A person holds at
/// most [`MAX_SESSIONS_PER_PERSON`] live sessions, plus those of sign-ins in
/// flight.
pub(super) const LINK_READ: usize = 4 * MAX_SESSIONS_PER_PERSON;

/// Keeps browser sessions in the table, by the digest of their secret.
#[derive(Debug, Clone)]
pub struct DynamoSessionStore {
    table: Table,
}

impl DynamoSessionStore {
    pub(crate) fn from_table(table: Table) -> Self {
        Self { table }
    }
}

fn answer<T>(answer: Final<T, SessionStoreError>) -> Result<T, SessionStoreError> {
    match answer {
        Final::Done(done) => Ok(done),
        Final::Refused(refused) => Err(refused),
        Final::Unavailable => Err(SessionStoreError::Unavailable),
    }
}

/// What a failed session write means. A condition on a session, a link or
/// the guard that failed means that another call changed what this one
/// read, so the call reads again.
pub(super) fn refusal<T>(classified: Classified) -> Sent<T, SessionStoreError> {
    match classified {
        Classified::Conflict {
            role:
                ItemRole::SessionGuard
                | ItemRole::OldSession
                | ItemRole::SessionLink
                | ItemRole::SessionItem,
            ..
        }
        | Classified::Retry => Sent::Again,
        Classified::Conflict { .. } | Classified::Unavailable(_) => Sent::Unavailable,
        Classified::Unknown => Sent::Unknown,
    }
}

async fn read_session(table: &Table, digest: &Digest) -> Result<Option<HeldSession>, ()> {
    let item = table
        .get(&Key::session(digest), ItemKind::Session)
        .await
        .map_err(|_| ())?;
    item.map(|item| session_from(&item))
        .transpose()
        .map_err(malformed)
}

/// The `rev` of `person`'s guard, which is `None` for a person with none.
async fn read_guard(table: &Table, person: &PrincipalId) -> Result<GuardRead, ()> {
    let item = table
        .get(&Key::session_guard(person), ItemKind::SessionGuard)
        .await
        .map_err(|_| ())?;
    item.map(|item| guard_rev(&item))
        .transpose()
        .map_err(malformed)
}

/// The expiries and digests of `person`'s links to sessions that expire
/// after `now`, earliest expiry first.
async fn live_of(
    table: &Table,
    person: &PrincipalId,
    now: Timestamp,
) -> Result<Vec<(Timestamp, Digest)>, ()> {
    let (low, high) = live_links(now);
    let items = table
        .query_between(
            &session_partition(person),
            (&low, &high),
            LINK_READ,
            ItemKind::SessionLink,
        )
        .await
        .map_err(|_| ())?;
    Ok(links_of(&items))
}

/// A sign-in.
struct CreateOp<'a> {
    table: &'a Table,
    session: &'a NewSession,
    replacing: Option<&'a Digest>,
    rule: &'a SessionRule,
    now: Timestamp,
}

/// A built sign-in: the transaction and the expiry it writes.
struct BuiltCreate {
    transaction: Transaction,
    expires: Timestamp,
}

impl Attempt for CreateOp<'_> {
    type Built = BuiltCreate;
    type Done = Timestamp;
    type Refused = SessionStoreError;

    async fn build(&self) -> Result<BuiltCreate, Sent<Timestamp, SessionStoreError>> {
        let person = self.session.principal.id();
        let unavailable = |()| Sent::Unavailable;
        let guard = read_guard(self.table, person).await.map_err(unavailable)?;
        let old = match self.replacing {
            Some(digest) => read_session(self.table, digest)
                .await
                .map_err(unavailable)?
                .filter(|held| held.expires > self.now)
                .map(|held| Ended {
                    digest: *digest,
                    person: held.principal.id().clone(),
                    expires: held.expires,
                }),
            None => None,
        };
        let live = live_of(self.table, person, self.now)
            .await
            .map_err(unavailable)?;
        let kept: Vec<_> = live
            .into_iter()
            .filter(|(_, digest)| old.as_ref().is_none_or(|old| old.digest != *digest))
            .collect();
        let surplus = kept
            .len()
            .saturating_sub(MAX_SESSIONS_PER_PERSON.saturating_sub(1));
        let mut ended: Vec<Ended> = kept
            .into_iter()
            .take(surplus)
            .map(|(expires, digest)| Ended {
                digest,
                person: person.clone(),
                expires,
            })
            .collect();
        ended.extend(old);
        let rev = Rev::fresh().ok_or(Sent::Unavailable)?;
        let expires = self.rule.first_expiry(self.now);
        let transaction = items::create(
            self.table.name(),
            &Creation {
                digest: &self.session.digest,
                principal: &self.session.principal,
                created: self.now,
                expires,
                guard_expires: Timestamp::from_secs(
                    self.now.as_secs().saturating_add(self.rule.cap()),
                ),
                guard: &guard,
                rev: &rev,
                ended: &ended,
            },
        )
        .ok_or(Sent::Unavailable)?;
        Ok(BuiltCreate {
            transaction,
            expires,
        })
    }

    async fn send(&self, built: &BuiltCreate, token: &Token) -> Sent<Timestamp, SessionStoreError> {
        match self
            .table
            .transact(&built.transaction, token, "create session")
            .await
        {
            Ok(()) => Sent::Done(built.expires),
            Err(classified) => refusal(classified),
        }
    }

    async fn settle(&self, built: &BuiltCreate) -> Settled<Timestamp> {
        match read_session(self.table, &self.session.digest).await {
            Ok(Some(held)) if held.principal == self.session.principal => {
                Settled::Matches(built.expires)
            }
            Ok(_) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

/// An end of one session.
struct RemoveOp<'a> {
    table: &'a Table,
    digest: &'a Digest,
}

impl Attempt for RemoveOp<'_> {
    type Built = Transaction;
    type Done = ();
    type Refused = SessionStoreError;

    async fn build(&self) -> Result<Transaction, Sent<(), SessionStoreError>> {
        let Some(held) = read_session(self.table, self.digest)
            .await
            .map_err(|()| Sent::Unavailable)?
        else {
            return Err(Sent::Done(()));
        };
        let ended = Ended {
            digest: *self.digest,
            person: held.principal.id().clone(),
            expires: held.expires,
        };
        items::end(self.table.name(), &ended).ok_or(Sent::Unavailable)
    }

    async fn send(&self, built: &Transaction, token: &Token) -> Sent<(), SessionStoreError> {
        match self.table.transact(built, token, "remove session").await {
            Ok(()) => Sent::Done(()),
            Err(classified) => refusal(classified),
        }
    }

    async fn settle(&self, _built: &Transaction) -> Settled<()> {
        match read_session(self.table, self.digest).await {
            Ok(None) => Settled::Matches(()),
            Ok(Some(_)) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

/// How many passes an end of all of a person's sessions makes, each of
/// which ends up to [`LINK_READ`] of them.
const END_ALL_PASSES: usize = 8;

impl SessionStore for DynamoSessionStore {
    async fn create(
        &self,
        session: NewSession,
        replacing: Option<Digest>,
        rule: &SessionRule,
        now: Timestamp,
    ) -> Result<Timestamp, SessionStoreError> {
        let op = CreateOp {
            table: &self.table,
            session: &session,
            replacing: replacing.as_ref(),
            rule,
            now,
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn principal(
        &self,
        digest: &Digest,
        rule: &SessionRule,
        now: Timestamp,
    ) -> Result<Option<Principal>, SessionStoreError> {
        let held = read_session(&self.table, digest)
            .await
            .map_err(|()| SessionStoreError::Unavailable)?
            .filter(|held| held.expires > now);
        let Some(held) = held else { return Ok(None) };
        if let Some(next) = rule.extended(held.created, held.expires, now)
            && let (Some(transaction), Some(token)) = (
                items::extend(self.table.name(), digest, &held, next),
                Token::fresh(),
            )
        {
            let sent = self
                .table
                .transact(&transaction, &token, "extend session")
                .await;
            if matches!(sent, Err(Classified::Conflict { .. } | Classified::Retry)) {
                // Another call changed the session: an extension that ran
                // first leaves it live, and a removal leaves nothing.
                let again = read_session(&self.table, digest)
                    .await
                    .map_err(|()| SessionStoreError::Unavailable)?
                    .filter(|held| held.expires > now);
                return Ok(again.map(|held| held.principal));
            }
        }
        Ok(Some(held.principal))
    }

    async fn expiry(
        &self,
        digest: &Digest,
        now: Timestamp,
    ) -> Result<Option<Timestamp>, SessionStoreError> {
        let held = read_session(&self.table, digest)
            .await
            .map_err(|()| SessionStoreError::Unavailable)?;
        Ok(held
            .map(|held| held.expires)
            .filter(|expires| *expires > now))
    }

    async fn holds(&self, person: &PrincipalId, now: Timestamp) -> Result<bool, SessionStoreError> {
        let live = live_of(&self.table, person, now)
            .await
            .map_err(|()| SessionStoreError::Unavailable)?;
        Ok(!live.is_empty())
    }

    async fn remove(&self, digest: &Digest) -> Result<(), SessionStoreError> {
        let op = RemoveOp {
            table: &self.table,
            digest,
        };
        answer(run(self.table.policy(), &op).await)
    }

    async fn remove_all_of(&self, person: &PrincipalId) -> Result<(), SessionStoreError> {
        let op = RemoveAllOp {
            table: &self.table,
            person,
        };
        for _ in 0..END_ALL_PASSES {
            if answer(run(self.table.policy(), &op).await)? == 0 {
                return Ok(());
            }
        }
        Err(SessionStoreError::Unavailable)
    }
}

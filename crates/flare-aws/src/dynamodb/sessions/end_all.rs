//! The pass of the end of all of one person's sessions.

use super::items::{self, links_of};
use super::{LINK_READ, refusal};
use crate::dynamodb::Table;
use crate::dynamodb::items::ItemKind;
use crate::dynamodb::keys::{SESSION_LINK_PREFIX, session_partition};
use crate::dynamodb::retry::{Attempt, Sent, Settled, Token};
use crate::dynamodb::transact::Transaction;
use flare_core::{Digest, PrincipalId, SessionStoreError, Timestamp};

/// One pass of the end of all of a person's sessions, which deletes up to
/// [`LINK_READ`] of them, expired or not, and the person's guard.
pub(super) struct RemoveAllOp<'a> {
    pub(super) table: &'a Table,
    pub(super) person: &'a PrincipalId,
}

impl RemoveAllOp<'_> {
    async fn links(&self, limit: usize) -> Result<Vec<(Timestamp, Digest)>, ()> {
        let items = self
            .table
            .query(
                &session_partition(self.person),
                SESSION_LINK_PREFIX,
                limit,
                ItemKind::SessionLink,
            )
            .await
            .map_err(|_| ())?;
        Ok(links_of(&items))
    }
}

impl Attempt for RemoveAllOp<'_> {
    type Built = (Transaction, usize);
    type Done = usize;
    type Refused = SessionStoreError;

    async fn build(&self) -> Result<(Transaction, usize), Sent<usize, SessionStoreError>> {
        let links = self
            .links(LINK_READ)
            .await
            .map_err(|()| Sent::Unavailable)?;
        let transaction =
            items::end_all(self.table.name(), self.person, &links).ok_or(Sent::Unavailable)?;
        Ok((transaction, links.len()))
    }

    async fn send(
        &self,
        built: &(Transaction, usize),
        token: &Token,
    ) -> Sent<usize, SessionStoreError> {
        match self
            .table
            .transact(&built.0, token, "remove sessions of a person")
            .await
        {
            Ok(()) => Sent::Done(built.1),
            Err(classified) => refusal(classified),
        }
    }

    async fn settle(&self, built: &(Transaction, usize)) -> Settled<usize> {
        match self.links(1).await {
            Ok(rest) if rest.is_empty() => Settled::Matches(built.1),
            Ok(_) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

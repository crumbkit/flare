//! [`DynamoResponseStore`], flare-core's `ResponseStore` on the table.

mod items;

use crate::dynamodb::Table;
use crate::dynamodb::cancel::Classified;
use crate::dynamodb::items::ItemKind;
use crate::dynamodb::keys::{Key, flare_partition, link_sort_key, response_partition};
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::dynamodb::settle::malformed;
use crate::dynamodb::transact::Transaction;
use flare_core::{NewLink, Position, ResponseLink, ResponseStore, ResponseStoreError, ShareId};
use items::{PAIRS, head_from, link_from};

/// The most links a read of one response's links returns. A response names
/// at most 16 flares when it is shared and 16 more with each grant, so a
/// response rarely comes near it. A response with more links is listed
/// with its first 1,000 only; reading the rest needs a paged read.
const MAX_RESPONSE_LINKS: usize = 1_000;

/// Keeps the links from responses to flares in the table, with one counter
/// that gives each link its position.
///
/// Every link moves the one counter, so links are written one at a time
/// across the whole table. A link that meets another link's move of the
/// counter reads the counter again and takes the next position.
#[derive(Debug, Clone)]
pub struct DynamoResponseStore {
    table: Table,
}

impl DynamoResponseStore {
    pub(crate) fn from_table(table: Table) -> Self {
        Self { table }
    }
}

async fn read_pair(
    table: &Table,
    response: &ShareId,
    flare: &ShareId,
) -> Result<Option<ResponseLink>, ()> {
    let item = table
        .get(&Key::pair(response, flare), ItemKind::Link)
        .await
        .map_err(|_| ())?;
    item.map(|item| link_from(&item))
        .transpose()
        .map_err(malformed)
}

async fn read_head(table: &Table) -> Result<Position, ()> {
    let item = table
        .get(&Key::sequence(), ItemKind::Sequence)
        .await
        .map_err(|_| ())?;
    head_from(item.as_ref()).map_err(malformed)
}

/// A link of one response to one flare.
struct LinkOp<'a> {
    table: &'a Table,
    link: &'a NewLink,
}

/// A built link: the transaction and the link it writes.
struct BuiltLink {
    transaction: Transaction,
    link: ResponseLink,
}

impl Attempt for LinkOp<'_> {
    type Built = BuiltLink;
    type Done = ResponseLink;
    type Refused = ResponseStoreError;

    async fn build(&self) -> Result<BuiltLink, Sent<ResponseLink, ResponseStoreError>> {
        match read_pair(self.table, &self.link.response, &self.link.flare).await {
            Ok(Some(first)) => return Err(Sent::Done(first)),
            Ok(None) => {}
            Err(()) => return Err(Sent::Unavailable),
        }
        let head = read_head(self.table)
            .await
            .map_err(|()| Sent::Unavailable)?;
        let next = head.get().checked_add(1).ok_or(Sent::Unavailable)?;
        let link = self.link.clone().at_position(Position::new(next));
        let transaction = items::link(self.table.name(), &link, head).ok_or(Sent::Unavailable)?;
        Ok(BuiltLink { transaction, link })
    }

    async fn send(
        &self,
        built: &BuiltLink,
        token: &Token,
    ) -> Sent<ResponseLink, ResponseStoreError> {
        match self.table.transact(&built.transaction, token, "link").await {
            Ok(()) => Sent::Done(built.link.clone()),
            // Another link moved the counter, or wrote this pair, since the
            // read; a fresh read takes the next position or finds the pair.
            Err(Classified::Conflict { .. } | Classified::Retry) => Sent::Again,
            Err(Classified::Unknown) => Sent::Unknown,
            Err(Classified::Unavailable(_)) => Sent::Unavailable,
        }
    }

    /// The pair item, whoever wrote it, is the first link of the pair,
    /// which a repeat answers.
    async fn settle(&self, _built: &BuiltLink) -> Settled<ResponseLink> {
        match read_pair(self.table, &self.link.response, &self.link.flare).await {
            Ok(Some(first)) => Settled::Matches(first),
            Ok(None) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

/// Reads links and turns a failure into the store's.
fn links(
    items: Result<Vec<crate::dynamodb::items::Item>, crate::dynamodb::read::Unavailable>,
) -> Result<Vec<ResponseLink>, ResponseStoreError> {
    items
        .map_err(|_| ResponseStoreError::Unavailable)?
        .iter()
        .map(link_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            malformed(error);
            ResponseStoreError::Unavailable
        })
}

impl ResponseStore for DynamoResponseStore {
    async fn link(&self, link: NewLink) -> Result<ResponseLink, ResponseStoreError> {
        let op = LinkOp {
            table: &self.table,
            link: &link,
        };
        match run(self.table.policy(), &op).await {
            Final::Done(done) => Ok(done),
            Final::Refused(refused) => Err(refused),
            Final::Unavailable => Err(ResponseStoreError::Unavailable),
        }
    }

    async fn head(&self) -> Result<Position, ResponseStoreError> {
        read_head(&self.table)
            .await
            .map_err(|()| ResponseStoreError::Unavailable)
    }

    async fn to_flare(
        &self,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> Result<Vec<ResponseLink>, ResponseStoreError> {
        let Some(low) = after.get().checked_add(1) else {
            return Ok(Vec::new());
        };
        if low > upto.get() {
            return Ok(Vec::new());
        }
        let low = link_sort_key(Position::new(low));
        let high = link_sort_key(upto);
        let read = self
            .table
            .query_between(
                &flare_partition(flare),
                (&low, &high),
                limit,
                ItemKind::Link,
            )
            .await;
        links(read)
    }

    async fn of_response(
        &self,
        response: &ShareId,
    ) -> Result<Vec<ResponseLink>, ResponseStoreError> {
        let read = self
            .table
            .query(
                &response_partition(response),
                PAIRS,
                MAX_RESPONSE_LINKS,
                ItemKind::Link,
            )
            .await;
        let mut found = links(read)?;
        found.sort_by_key(|link| link.position);
        Ok(found)
    }
}

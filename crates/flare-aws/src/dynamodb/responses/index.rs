//! The activity index on the table: the writes of entries and their
//! markers, their removal, and a reader's list on one flare.
//!
//! An index write reads the markers of its entries, drops the entries
//! already written, reads the head and writes the rest in one transaction
//! that moves the counter by their number. A transaction holds at most
//! [`ENTRIES_PER_TRANSACTION`] entries, so a larger write is several
//! transactions in turn, and a failure partway leaves the earlier ones
//! written; a repeat skips them by their markers.

use super::items::{self, entry_from};
use super::read_head;
use crate::dynamodb::Table;
use crate::dynamodb::cancel::Classified;
use crate::dynamodb::items::ItemKind;
use crate::dynamodb::keys::{Key, index_entry_sort_key, index_markers_of, index_partition};
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::dynamodb::settle::malformed;
use crate::dynamodb::transact::Transaction;
use flare_core::{IndexEntry, NewEntry, Position, Reader, ResponseStoreError, ShareId};

/// The most entries one transaction writes. With a marker for each and the
/// counter, that is 99 items of DynamoDB's 100.
const ENTRIES_PER_TRANSACTION: usize = 49;

/// The most markers a removal reads for one reader and response: one per
/// flare the response links to, which is at most the links a response's
/// list reads.
const MAX_MARKERS: usize = 1_000;

/// Whether `table` holds the marker of `entry`.
async fn marked(table: &Table, entry: &NewEntry) -> Result<bool, ()> {
    let key = Key::index_marker(&entry.reader, &entry.link.response, &entry.link.flare);
    let item = table
        .get(&key, ItemKind::IndexMarker)
        .await
        .map_err(|_| ())?;
    Ok(item.is_some())
}

/// One transaction's worth of entries to write.
struct IndexOp<'a> {
    table: &'a Table,
    entries: &'a [NewEntry],
}

impl Attempt for IndexOp<'_> {
    type Built = Transaction;
    type Done = ();
    type Refused = ResponseStoreError;

    async fn build(&self) -> Result<Transaction, Sent<(), ResponseStoreError>> {
        let mut missing = Vec::new();
        for entry in self.entries {
            if !marked(self.table, entry)
                .await
                .map_err(|()| Sent::Unavailable)?
            {
                missing.push(entry.clone());
            }
        }
        if missing.is_empty() {
            return Err(Sent::Done(()));
        }
        let head = read_head(self.table)
            .await
            .map_err(|()| Sent::Unavailable)?;
        let mut next = head.get();
        let mut stored = Vec::with_capacity(missing.len());
        for entry in missing {
            next = next.checked_add(1).ok_or(Sent::Unavailable)?;
            stored.push(entry.at_position(Position::new(next)));
        }
        items::index(self.table.name(), &stored, head).ok_or(Sent::Unavailable)
    }

    async fn send(&self, built: &Transaction, token: &Token) -> Sent<(), ResponseStoreError> {
        match self.table.transact(built, token, "index").await {
            Ok(()) => Sent::Done(()),
            // Another writer moved the counter or wrote one of these
            // markers since the read; a fresh read skips what it wrote and
            // takes the next positions.
            Err(Classified::Conflict { .. } | Classified::Retry) => Sent::Again,
            Err(Classified::Unknown) => Sent::Unknown,
            Err(Classified::Unavailable(_)) => Sent::Unavailable,
        }
    }

    /// Every marker present means every entry is written, by this attempt
    /// or another; a missing one sends the loop to write it.
    async fn settle(&self, _built: &Transaction) -> Settled<()> {
        for entry in self.entries {
            match marked(self.table, entry).await {
                Ok(true) => {}
                Ok(false) => return Settled::Differs,
                Err(()) => return Settled::Unavailable,
            }
        }
        Settled::Matches(())
    }
}

/// The removal of one entry and its marker.
struct UnindexOp<'a> {
    table: &'a Table,
    entry: &'a IndexEntry,
}

impl Attempt for UnindexOp<'_> {
    type Built = Transaction;
    type Done = ();
    type Refused = ResponseStoreError;

    async fn build(&self) -> Result<Transaction, Sent<(), ResponseStoreError>> {
        items::unindex(self.table.name(), self.entry).ok_or(Sent::Unavailable)
    }

    async fn send(&self, built: &Transaction, token: &Token) -> Sent<(), ResponseStoreError> {
        match self.table.transact(built, token, "unindex").await {
            Ok(()) => Sent::Done(()),
            Err(Classified::Conflict { .. } | Classified::Retry) => Sent::Again,
            Err(Classified::Unknown) => Sent::Unknown,
            Err(Classified::Unavailable(_)) => Sent::Unavailable,
        }
    }

    async fn settle(&self, _built: &Transaction) -> Settled<()> {
        let key = Key::index_marker(&self.entry.reader, &self.entry.response, &self.entry.flare);
        match self.table.get(&key, ItemKind::IndexMarker).await {
            Ok(None) => Settled::Matches(()),
            Ok(Some(_)) => Settled::Differs,
            Err(_) => Settled::Unavailable,
        }
    }
}

fn answered(outcome: Final<(), ResponseStoreError>) -> Result<(), ResponseStoreError> {
    match outcome {
        Final::Done(()) => Ok(()),
        Final::Refused(refused) => Err(refused),
        Final::Unavailable => Err(ResponseStoreError::Unavailable),
    }
}

/// Writes `entries`, once each per reader, response and flare.
pub(super) async fn index(table: &Table, entries: Vec<NewEntry>) -> Result<(), ResponseStoreError> {
    let mut distinct: Vec<NewEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        let repeat = distinct.iter().any(|seen| {
            seen.reader == entry.reader
                && seen.link.response == entry.link.response
                && seen.link.flare == entry.link.flare
        });
        if !repeat {
            distinct.push(entry);
        }
    }
    for chunk in distinct.chunks(ENTRIES_PER_TRANSACTION) {
        let op = IndexOp {
            table,
            entries: chunk,
        };
        answered(run(table.policy(), &op).await)?;
    }
    Ok(())
}

/// Removes every entry of `reader` for `response`, one transaction per
/// flare.
pub(super) async fn unindex(
    table: &Table,
    reader: &Reader,
    response: &ShareId,
) -> Result<(), ResponseStoreError> {
    let markers = table
        .query(
            &index_partition(reader),
            &index_markers_of(response),
            MAX_MARKERS,
            ItemKind::IndexMarker,
        )
        .await
        .map_err(|_| ResponseStoreError::Unavailable)?;
    for marker in &markers {
        let entry = entry_from(marker, reader, ItemKind::IndexMarker).map_err(|error| {
            malformed(error);
            ResponseStoreError::Unavailable
        })?;
        let op = UnindexOp {
            table,
            entry: &entry,
        };
        answered(run(table.policy(), &op).await)?;
    }
    Ok(())
}

/// The entries of `reader` on `flare` above `after` and at most `upto`,
/// oldest first, at most `limit`.
pub(super) async fn visible(
    table: &Table,
    reader: &Reader,
    flare: &ShareId,
    (after, upto): (Position, Position),
    limit: usize,
) -> Result<Vec<IndexEntry>, ResponseStoreError> {
    let Some(low) = after.get().checked_add(1) else {
        return Ok(Vec::new());
    };
    if low > upto.get() {
        return Ok(Vec::new());
    }
    let low = index_entry_sort_key(flare, Position::new(low));
    let high = index_entry_sort_key(flare, upto);
    let read = table
        .query_between(
            &index_partition(reader),
            (&low, &high),
            limit,
            ItemKind::IndexEntry,
        )
        .await
        .map_err(|_| ResponseStoreError::Unavailable)?;
    read.iter()
        .map(|item| entry_from(item, reader, ItemKind::IndexEntry))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            malformed(error);
            ResponseStoreError::Unavailable
        })
}

//! The identity marker: one item, `IDENTITY` `IDENTITY`, whose `kind` is
//! the identifier kind the table was first written under, so a start under
//! the other kind is refused instead of leaving every grant matching
//! nobody.
//!
//! | State | Entered when | Leaves to |
//! | --- | --- | --- |
//! | absent | the table is new, or was written before the marker | email or username |
//! | email | a start finds no marker and either runs under the email kind or finds the table already holds items | none |
//! | username | a start under the username kind finds no marker and an empty table | none |
//!
//! The marker is put only if absent, so two starts at once leave one
//! winner, and the loser reads it and compares. No call removes or changes
//! the marker. A crash between the empty-table check and the put leaves no
//! marker, and the next start repeats both steps.

use crate::dynamodb::cancel::Classified;
use crate::dynamodb::items::{Item, ItemKind, Malformed, attr, keyed, read_text, text};
use crate::dynamodb::keys::Key;
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::dynamodb::settle::malformed;
use crate::dynamodb::transact::{ABSENT, ItemRole, Transaction, put};
use crate::dynamodb::{Table, error_code};
use flare_core::IdentityKind;

/// Why a table's identity could not be settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IdentityError {
    /// The table was first written under another identifier kind. Nothing
    /// was written.
    #[error("the table names people by {}", recorded.as_str())]
    Changed {
        /// The kind the table's marker records.
        recorded: IdentityKind,
    },
    /// The table could not be read or written. A retry may succeed.
    #[error("the table's identity marker could not be read or written")]
    Unavailable,
}

fn marker_item(kind: IdentityKind) -> Item {
    keyed(&Key::identity(), [(attr::KIND, text(kind.as_str()))])
}

fn marker_from(item: &Item) -> Result<IdentityKind, Malformed> {
    let kind = ItemKind::Identity;
    IdentityKind::try_from(read_text(item, attr::KIND, kind)?).map_err(|_| Malformed(kind))
}

/// The kind the marker records, or `None` when the table has no marker.
async fn read_marker(table: &Table) -> Result<Option<IdentityKind>, ()> {
    let item = table
        .get(&Key::identity(), ItemKind::Identity)
        .await
        .map_err(|_| ())?;
    item.map(|item| marker_from(&item))
        .transpose()
        .map_err(malformed)
}

/// Whether the table holds any item. The marker is read as absent first,
/// so any item is data written before it.
async fn holds_items(table: &Table) -> Result<bool, ()> {
    let scanned = table
        .client()
        .scan()
        .table_name(table.name())
        .limit(1)
        .consistent_read(true)
        .send()
        .await;
    match scanned {
        Ok(output) => Ok(!output.items().is_empty()),
        Err(error) => {
            tracing::warn!(
                call = "Scan",
                code = error_code(&error).as_str(),
                "the table could not be scanned for items written before the identity marker"
            );
            Err(())
        }
    }
}

/// A call that records the marker when it is absent and answers the kind
/// the marker holds.
struct MarkOp<'a> {
    table: &'a Table,
    kind: IdentityKind,
}

/// A built write: the marker put and the kind it records.
struct BuiltMarker {
    transaction: Transaction,
    kind: IdentityKind,
}

impl Attempt for MarkOp<'_> {
    type Built = BuiltMarker;
    type Done = IdentityKind;
    type Refused = IdentityError;

    async fn build(&self) -> Result<BuiltMarker, Sent<IdentityKind, IdentityError>> {
        match read_marker(self.table).await {
            Ok(Some(recorded)) => return Err(Sent::Done(recorded)),
            Ok(None) => {}
            Err(()) => return Err(Sent::Unavailable),
        }
        // A table that holds items but no marker was written before the
        // marker, when every deployment named people by email.
        let kind = if holds_items(self.table)
            .await
            .map_err(|()| Sent::Unavailable)?
        {
            IdentityKind::Email
        } else {
            self.kind
        };
        let mut transaction = Transaction::default();
        let item =
            put(self.table.name(), marker_item(kind), Some(ABSENT)).ok_or(Sent::Unavailable)?;
        transaction.push(ItemRole::IdentityMarker, item);
        Ok(BuiltMarker { transaction, kind })
    }

    async fn send(&self, built: &BuiltMarker, token: &Token) -> Sent<IdentityKind, IdentityError> {
        match self
            .table
            .transact(&built.transaction, token, "write identity marker")
            .await
        {
            Ok(()) => Sent::Done(built.kind),
            // Another start wrote the marker first, so the call reads it.
            Err(
                Classified::Conflict {
                    role: ItemRole::IdentityMarker,
                    ..
                }
                | Classified::Retry,
            ) => Sent::Again,
            Err(Classified::Unknown) => Sent::Unknown,
            Err(_) => Sent::Unavailable,
        }
    }

    async fn settle(&self, built: &BuiltMarker) -> Settled<IdentityKind> {
        match read_marker(self.table).await {
            Ok(Some(recorded)) if recorded == built.kind => Settled::Matches(recorded),
            Ok(_) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

/// Records `kind` in the table's marker when it has none, and checks that
/// the marker records `kind`.
pub(crate) async fn settle(table: &Table, kind: IdentityKind) -> Result<(), IdentityError> {
    let op = MarkOp { table, kind };
    let recorded = match run(table.policy(), &op).await {
        Final::Done(recorded) => recorded,
        Final::Refused(refused) => return Err(refused),
        Final::Unavailable => return Err(IdentityError::Unavailable),
    };
    if recorded == kind {
        Ok(())
    } else {
        Err(IdentityError::Changed { recorded })
    }
}

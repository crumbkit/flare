//! The teams marker: one item, `TEAMS` `TEAMS`, whose `kind` says whether
//! the table's team grants were made under flare's stored teams or under a
//! deployment's directory teams. A team grant names only the team, so a
//! start under the other teams would open each grant to whichever team of
//! that name the other source holds. A start under the other teams is
//! therefore refused while any team grant remains, and recorded once none
//! does.
//!
//! | State | Entered when | Leaves to |
//! | --- | --- | --- |
//! | absent | the table is new, or was written before the marker | stored or directory |
//! | stored | a start under stored teams finds no marker, or finds no team grant | directory |
//! | directory | a start under directory teams finds no marker and no team grant, or finds no team grant | stored |
//!
//! A table with team grants and no marker was written before the marker,
//! when every deployment used stored teams, so a directory start on it is
//! refused and a stored start records stored. The marker is put only if
//! it still holds what the start read, so two starts at once leave one
//! winner, and the loser reads it again. A crash between the grant scan and
//! the put leaves the marker as it was, and the next start repeats both
//! steps. A process that still serves under the old teams may write a team
//! grant after a start under the new teams has recorded them, so every
//! process under the old teams stops before one starts under the new.
//!
//! The identity marker is settled first, because its check for items
//! written before it counts every item, this marker included.

use crate::dynamodb::cancel::Classified;
use crate::dynamodb::items::{Item, ItemKind, Malformed, attr, keyed, read_text, text};
use crate::dynamodb::keys::{GRANT_PREFIX, Key, share_from_partition};
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::dynamodb::settle::malformed;
use crate::dynamodb::transact::{ABSENT, ItemRole, Transaction};
use crate::dynamodb::{Table, error_code};
use aws_sdk_dynamodb::types::{Put, TransactWriteItem};
use flare_core::{Group, ShareId, TeamName};

/// The teams a table's team grants were made under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TeamsKind {
    /// flare's own teams, kept in the table.
    Stored,
    /// The deployment's directory teams, read from its teams document.
    Directory,
}

impl TeamsKind {
    /// The kind as the marker spells it: `stored` or `directory`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stored => "stored",
            Self::Directory => "directory",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "stored" => Some(Self::Stored),
            "directory" => Some(Self::Directory),
            _ => None,
        }
    }
}

/// Why a table's teams could not be settled.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TeamsKindError {
    /// The table holds a team grant made under other teams. Nothing was
    /// written.
    #[error(
        "the table holds team grants made under {} teams, such as one to team:{team} on {}",
        recorded.as_str(),
        share.as_str()
    )]
    Changed {
        /// The teams the table records.
        recorded: TeamsKind,
        /// The share of the first team grant the scan found.
        share: ShareId,
        /// That grant's team.
        team: TeamName,
    },
    /// The table could not be read or written. A retry may succeed.
    #[error("the table's teams marker could not be read or written")]
    Unavailable,
}

/// How a team grantee's id starts, as [`Group`] spells it. A grant the
/// scan finds is parsed with [`Group::parse`], so a different spelling
/// reads as a malformed item.
const TEAM_GRANTEE: &str = "team:";

/// A team grant: its share and its team.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TeamGrant {
    share: ShareId,
    team: TeamName,
}

fn marker_item(kind: TeamsKind) -> Item {
    keyed(&Key::teams(), [(attr::KIND, text(kind.as_str()))])
}

fn marker_from(item: &Item) -> Result<TeamsKind, Malformed> {
    let kind = ItemKind::TeamsMarker;
    TeamsKind::parse(read_text(item, attr::KIND, kind)?).ok_or(Malformed(kind))
}

/// The kind the marker records, or `None` when the table has no marker.
async fn read_marker(table: &Table) -> Result<Option<TeamsKind>, ()> {
    let item = table
        .get(&Key::teams(), ItemKind::TeamsMarker)
        .await
        .map_err(|_| ())?;
    item.map(|item| marker_from(&item))
        .transpose()
        .map_err(malformed)
}

/// The team grant that `item`, a grant item the scan found, holds.
fn team_grant(item: &Item) -> Result<TeamGrant, Malformed> {
    let kind = ItemKind::Grant;
    let share = share_from_partition(read_text(item, attr::PK, kind)?).ok_or(Malformed(kind))?;
    let grantee = read_text(item, attr::SK, kind)?
        .strip_prefix(GRANT_PREFIX)
        .ok_or(Malformed(kind))?;
    match Group::parse(grantee) {
        Some(Ok(Group::Team(team))) => Ok(TeamGrant { share, team }),
        _ => Err(Malformed(kind)),
    }
}

/// The first team grant a scan of the whole table finds, or `None` when
/// the table holds none. The scan reads every item, page by page, and
/// runs only when the marker is absent or records other teams.
async fn first_team_grant(table: &Table) -> Result<Option<TeamGrant>, ()> {
    let prefix = format!("{GRANT_PREFIX}{TEAM_GRANTEE}");
    let mut start = None;
    loop {
        let scanned = table
            .client()
            .scan()
            .table_name(table.name())
            .consistent_read(true)
            .filter_expression("begins_with(#sk, :prefix)")
            .expression_attribute_names("#sk", attr::SK)
            .expression_attribute_names("#pk", attr::PK)
            .expression_attribute_values(":prefix", text(&prefix))
            .projection_expression("#pk, #sk")
            .set_exclusive_start_key(start)
            .send()
            .await;
        let output = match scanned {
            Ok(output) => output,
            Err(error) => {
                tracing::warn!(
                    call = "Scan",
                    code = error_code(&error).as_str(),
                    "the table could not be scanned for team grants"
                );
                return Err(());
            }
        };
        if let Some(item) = output.items().first() {
            return team_grant(item).map(Some).map_err(malformed);
        }
        match output.last_evaluated_key {
            Some(key) if !key.is_empty() => start = Some(key),
            _ => return Ok(None),
        }
    }
}

/// A call that records the marker when no team grant stands in the way,
/// and answers the kind the marker then holds.
struct MarkOp<'a> {
    table: &'a Table,
    kind: TeamsKind,
}

/// A built write: the marker put and the kind it records.
struct BuiltMarker {
    transaction: Transaction,
    kind: TeamsKind,
}

/// The put of the marker as `kind`, only while the marker holds `read`.
fn marker_put(table: &str, kind: TeamsKind, read: Option<TeamsKind>) -> Option<TransactWriteItem> {
    let put = Put::builder()
        .table_name(table)
        .set_item(Some(marker_item(kind)));
    let put = match read {
        None => put.condition_expression(ABSENT),
        Some(read) => put
            .condition_expression("#kind = :read")
            .expression_attribute_names("#kind", attr::KIND)
            .expression_attribute_values(":read", text(read.as_str())),
    };
    Some(TransactWriteItem::builder().put(put.build().ok()?).build())
}

impl Attempt for MarkOp<'_> {
    type Built = BuiltMarker;
    type Done = TeamsKind;
    type Refused = TeamsKindError;

    async fn build(&self) -> Result<BuiltMarker, Sent<TeamsKind, TeamsKindError>> {
        let read = match read_marker(self.table).await {
            Ok(Some(recorded)) if recorded == self.kind => return Err(Sent::Done(recorded)),
            Ok(read) => read,
            Err(()) => return Err(Sent::Unavailable),
        };
        let found = first_team_grant(self.table)
            .await
            .map_err(|()| Sent::Unavailable)?;
        if let Some(TeamGrant { share, team }) = found {
            // A table with team grants and no marker was written before the
            // marker, when every deployment used stored teams.
            let recorded = read.unwrap_or(TeamsKind::Stored);
            if recorded != self.kind {
                return Err(Sent::Refused(TeamsKindError::Changed {
                    recorded,
                    share,
                    team,
                }));
            }
        }
        let mut transaction = Transaction::default();
        let item = marker_put(self.table.name(), self.kind, read).ok_or(Sent::Unavailable)?;
        transaction.push(ItemRole::TeamsMarker, item);
        Ok(BuiltMarker {
            transaction,
            kind: self.kind,
        })
    }

    async fn send(&self, built: &BuiltMarker, token: &Token) -> Sent<TeamsKind, TeamsKindError> {
        match self
            .table
            .transact(&built.transaction, token, "write teams marker")
            .await
        {
            Ok(()) => Sent::Done(built.kind),
            // Another start changed the marker first, so the call reads it.
            Err(
                Classified::Conflict {
                    role: ItemRole::TeamsMarker,
                    ..
                }
                | Classified::Retry,
            ) => Sent::Again,
            Err(Classified::Unknown) => Sent::Unknown,
            Err(_) => Sent::Unavailable,
        }
    }

    async fn settle(&self, built: &BuiltMarker) -> Settled<TeamsKind> {
        match read_marker(self.table).await {
            Ok(Some(recorded)) if recorded == built.kind => Settled::Matches(recorded),
            Ok(_) => Settled::Differs,
            Err(()) => Settled::Unavailable,
        }
    }
}

/// Records `kind` in the table's teams marker unless a team grant made
/// under other teams stands in the way, and checks that the marker then
/// records `kind`.
pub(crate) async fn settle(table: &Table, kind: TeamsKind) -> Result<(), TeamsKindError> {
    let op = MarkOp { table, kind };
    match run(table.policy(), &op).await {
        Final::Done(recorded) if recorded == kind => Ok(()),
        // The marker changed to other teams between the build and the
        // send's last read; the next start scans again.
        Final::Done(_) | Final::Unavailable => Err(TeamsKindError::Unavailable),
        Final::Refused(refused) => Err(refused),
    }
}

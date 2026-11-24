//! [`DynamoTeamStore`], flare-core's `TeamStore` on the table.
//!
//! A team is one item, `TEAM#<name>` `META`, with its owner, creation
//! time, state and member count. A member is a pair of items written and
//! deleted in one transaction: `TEAM#<name>` `M#<email>`, which lists a
//! team's members, and `MEMBER#<email>` `T#<name>`, which lists a person's
//! teams. A create puts the team's item only if absent, so a name in use
//! is refused. A deleting team keeps its item until its last member pair
//! is gone, and `forget` then deletes the item, which frees the name. An
//! add raises the count only while the team is active and below the member
//! limit. A transfer changes the owner only while the team is active, the
//! old owner still owns it and the new owner's member pair is present.

mod items;

use crate::dynamodb::Table;
use crate::dynamodb::cancel::Classified;
use crate::dynamodb::items::ItemKind;
use crate::dynamodb::keys::{
    Key, MEMBER_PREFIX, TEAM_PREFIX, member_of, member_partition, team_of, team_partition,
};
use crate::dynamodb::retry::{Attempt, Final, Sent, Settled, Token, run};
use crate::dynamodb::settle::malformed;
use crate::dynamodb::transact::{ItemRole, Transaction};
use flare_core::{Email, MAX_MEMBERS, Team, TeamName, TeamState, TeamStore, TeamStoreError};

/// The most entries a person's team list reads. The service refuses a
/// person a team beyond 32, and this bound leaves room for teams that a
/// race added.
const MAX_TEAMS_READ: usize = 256;

/// Keeps teams and their members in the table.
#[derive(Debug, Clone)]
pub struct DynamoTeamStore {
    table: Table,
}

impl DynamoTeamStore {
    pub(crate) fn from_table(table: Table) -> Self {
        Self { table }
    }
}

/// Reads the team with the name, active or deleted.
async fn read_team(table: &Table, name: &TeamName) -> Result<Option<Team>, TeamStoreError> {
    let item = table
        .get(&Key::team(name), ItemKind::Team)
        .await
        .map_err(|_| TeamStoreError::Unavailable)?;
    item.map(|item| items::team_from(&item, name))
        .transpose()
        .map_err(|error| {
            malformed(error);
            TeamStoreError::Unavailable
        })
}

/// Whether the pair of `member` in `team` is present.
async fn has_pair(table: &Table, team: &TeamName, member: &Email) -> Result<bool, TeamStoreError> {
    let (in_team, _) = Key::member_pair(team, member);
    table
        .get(&in_team, ItemKind::Member)
        .await
        .map(|item| item.is_some())
        .map_err(|_| TeamStoreError::Unavailable)
}

/// What a failed team transaction means: a taken name for a create, and
/// otherwise a reason to read again, because a fresh read tells a deleted,
/// full or already changed team apart.
fn refusal<T>(classified: Classified, creating: bool) -> Sent<T, TeamStoreError> {
    match classified {
        Classified::Conflict {
            role: ItemRole::TeamItem,
            ..
        } if creating => Sent::Refused(TeamStoreError::Taken),
        Classified::Conflict {
            role: ItemRole::TeamItem | ItemRole::MemberItem,
            ..
        }
        | Classified::Retry => Sent::Again,
        Classified::Conflict { .. } | Classified::Unavailable(_) => Sent::Unavailable,
        Classified::Unknown => Sent::Unknown,
    }
}

fn answer<T>(answer: Final<T, TeamStoreError>) -> Result<T, TeamStoreError> {
    match answer {
        Final::Done(done) => Ok(done),
        Final::Refused(refused) => Err(refused),
        Final::Unavailable => Err(TeamStoreError::Unavailable),
    }
}

/// The settled answer of a read that `matches` judges.
fn settled<T>(read: Result<bool, TeamStoreError>, done: T) -> Settled<T> {
    match read {
        Ok(true) => Settled::Matches(done),
        Ok(false) => Settled::Differs,
        Err(_) => Settled::Unavailable,
    }
}

/// The six writes of a team store.
#[derive(Debug)]
enum Op<'a> {
    Create(&'a Team),
    Add(&'a TeamName, &'a Email),
    Remove(&'a TeamName, &'a Email),
    MarkDeleting(&'a TeamName),
    Forget(&'a TeamName),
    Transfer(&'a TeamName, &'a Email, &'a Email),
}

/// One write with the table it goes to.
#[derive(Debug)]
struct TeamOp<'a> {
    table: &'a Table,
    op: Op<'a>,
}

impl Attempt for TeamOp<'_> {
    type Built = Transaction;
    type Done = bool;
    type Refused = TeamStoreError;

    async fn build(&self) -> Result<Transaction, Sent<bool, TeamStoreError>> {
        let table = self.table;
        let name = self.table.name();
        let unavailable = |_| Sent::Unavailable;
        let built = match self.op {
            Op::Create(team) => items::create(name, team),
            Op::Add(team, member) => {
                let found = read_team(table, team).await.map_err(unavailable)?;
                let Some(found) = found.filter(|found| found.state() == TeamState::Active) else {
                    return Err(Sent::Refused(TeamStoreError::NotFound));
                };
                if has_pair(table, team, member).await.map_err(unavailable)? {
                    return Err(Sent::Done(false));
                }
                if found.members() >= MAX_MEMBERS {
                    return Err(Sent::Refused(TeamStoreError::Full));
                }
                items::add(name, team, member)
            }
            Op::Remove(team, member) => {
                if !has_pair(table, team, member).await.map_err(unavailable)? {
                    return Err(Sent::Done(false));
                }
                items::remove(name, team, member)
            }
            Op::MarkDeleting(team) => match read_team(table, team).await.map_err(unavailable)? {
                None => return Err(Sent::Refused(TeamStoreError::NotFound)),
                Some(found) if found.state() == TeamState::Deleting => {
                    return Err(Sent::Done(true));
                }
                Some(_) => items::mark_deleting(name, team),
            },
            Op::Forget(team) => match read_team(table, team).await.map_err(unavailable)? {
                None => return Err(Sent::Done(false)),
                Some(found) if found.state() == TeamState::Deleting && found.members() == 0 => {
                    items::forget(name, team)
                }
                Some(_) => return Err(Sent::Refused(TeamStoreError::NotFound)),
            },
            Op::Transfer(team, from, to) => {
                let found = read_team(table, team).await.map_err(unavailable)?;
                let owned = found.is_some_and(|found| {
                    found.state() == TeamState::Active && found.owner() == from
                });
                if !owned || !has_pair(table, team, to).await.map_err(unavailable)? {
                    return Err(Sent::Refused(TeamStoreError::NotFound));
                }
                items::transfer(name, team, from, to)
            }
        };
        built.ok_or(Sent::Unavailable)
    }

    async fn send(&self, built: &Transaction, token: &Token) -> Sent<bool, TeamStoreError> {
        match self.table.transact(built, token, "team").await {
            Ok(()) => Sent::Done(true),
            Err(classified) => refusal(classified, matches!(self.op, Op::Create(_))),
        }
    }

    async fn settle(&self, _built: &Transaction) -> Settled<bool> {
        let table = self.table;
        match self.op {
            Op::Create(team) => {
                let read = read_team(table, team.name()).await.map(|found| {
                    found.is_some_and(|found| {
                        found.owner() == team.owner() && found.created() == team.created()
                    })
                });
                settled(read, true)
            }
            Op::Add(team, member) => settled(has_pair(table, team, member).await, true),
            Op::Remove(team, member) => settled(
                has_pair(table, team, member).await.map(|present| !present),
                true,
            ),
            Op::MarkDeleting(team) => {
                let read = read_team(table, team)
                    .await
                    .map(|found| found.is_some_and(|found| found.state() == TeamState::Deleting));
                settled(read, true)
            }
            Op::Forget(team) => settled(
                read_team(table, team).await.map(|found| found.is_none()),
                true,
            ),
            Op::Transfer(team, _, to) => {
                let read = read_team(table, team)
                    .await
                    .map(|found| found.is_some_and(|found| found.owner() == to));
                settled(read, true)
            }
        }
    }
}

impl DynamoTeamStore {
    async fn write(&self, op: Op<'_>) -> Result<bool, TeamStoreError> {
        let attempt = TeamOp {
            table: &self.table,
            op,
        };
        answer(run(self.table.policy(), &attempt).await)
    }
}

impl TeamStore for DynamoTeamStore {
    async fn create(&self, team: &Team) -> Result<(), TeamStoreError> {
        self.write(Op::Create(team)).await.map(|_| ())
    }

    async fn get(&self, name: &TeamName) -> Result<Option<Team>, TeamStoreError> {
        read_team(&self.table, name).await
    }

    async fn add(&self, name: &TeamName, member: &Email) -> Result<bool, TeamStoreError> {
        self.write(Op::Add(name, member)).await
    }

    async fn remove(&self, name: &TeamName, member: &Email) -> Result<bool, TeamStoreError> {
        self.write(Op::Remove(name, member)).await
    }

    async fn mark_deleting(&self, name: &TeamName) -> Result<(), TeamStoreError> {
        self.write(Op::MarkDeleting(name)).await.map(|_| ())
    }

    async fn forget(&self, name: &TeamName) -> Result<bool, TeamStoreError> {
        self.write(Op::Forget(name)).await
    }

    async fn transfer(
        &self,
        name: &TeamName,
        from: &Email,
        to: &Email,
    ) -> Result<(), TeamStoreError> {
        self.write(Op::Transfer(name, from, to)).await.map(|_| ())
    }

    async fn members(&self, name: &TeamName) -> Result<Vec<Email>, TeamStoreError> {
        // One item more than a team may hold shows a count gone wrong.
        let items = self
            .table
            .query(
                &team_partition(name),
                MEMBER_PREFIX,
                MAX_MEMBERS + 1,
                None,
                ItemKind::Member,
            )
            .await
            .map_err(|_| TeamStoreError::Unavailable)?;
        items
            .iter()
            .map(|item| sort_key(item).and_then(member_of))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                malformed(crate::dynamodb::items::Malformed(ItemKind::Member));
                TeamStoreError::Unavailable
            })
    }

    async fn teams_of(&self, member: &Email) -> Result<Vec<TeamName>, TeamStoreError> {
        let items = self
            .table
            .query(
                &member_partition(member),
                TEAM_PREFIX,
                MAX_TEAMS_READ,
                None,
                ItemKind::Member,
            )
            .await
            .map_err(|_| TeamStoreError::Unavailable)?;
        items
            .iter()
            .map(|item| sort_key(item).and_then(team_of))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                malformed(crate::dynamodb::items::Malformed(ItemKind::Member));
                TeamStoreError::Unavailable
            })
    }
}

/// An item's sort key.
fn sort_key(item: &crate::dynamodb::items::Item) -> Option<&str> {
    match item.get(crate::dynamodb::items::attr::SK) {
        Some(aws_sdk_dynamodb::types::AttributeValue::S(key)) => Some(key),
        _ => None,
    }
}

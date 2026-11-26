//! [`Directory`], the teams of one teams document once its limits are
//! checked, and [`DocumentRefused`], why a document was refused.
//!
//! A document is a JSON object whose one key, `teams`, maps each team's
//! name to the list of its members, such as
//! `{"teams": {"design": ["ana@example.com"]}}`. Each member is a person of
//! the deployment's identifier kind: an address, or a username.

use super::MAX_TEAMS_JOINED;
use super::names::TeamName;
use crate::person::{IdentityKind, Person};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

/// The most bytes a teams document holds. A longer one is refused.
pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

/// The most teams a teams document lists. A document with more is
/// refused.
pub const MAX_DIRECTORY_TEAMS: usize = 2_000;

/// The most members one team of a teams document has. A document with a
/// larger team is refused.
pub const MAX_DIRECTORY_MEMBERS: usize = 5_000;

/// Why a teams document could not be read, such as a missing file or a
/// refused request. The reason never holds the document's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentUnreadable(String);

impl DocumentUnreadable {
    /// The reason `reason`.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }

    /// The reason.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DocumentUnreadable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Why a teams document was refused. No variant holds a member's address
/// or username: a member is named by its team and its position in the
/// team's list, counted from 1.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DocumentRefused {
    /// The document could not be read.
    #[error("the teams document could not be read: {0}")]
    Unreadable(DocumentUnreadable),
    /// The document has more than [`MAX_DOCUMENT_BYTES`] bytes.
    #[error("the teams document is longer than {MAX_DOCUMENT_BYTES} bytes")]
    TooLarge,
    /// The document is not JSON, or not an object whose one key `teams`
    /// maps each team's name to a list of members.
    #[error(
        "the teams document is not an object whose one key, teams, maps each team's name to a list of its members (line {line}, column {column})"
    )]
    Malformed {
        /// The line where reading stopped.
        line: usize,
        /// The column where reading stopped.
        column: usize,
    },
    /// The team at `position` in name order has a name that is not 1 to
    /// 40 lowercase letters, digits and dashes.
    #[error(
        "team {position} of the teams document, in name order, has a name that is not 1 to 40 lowercase letters, digits and dashes"
    )]
    TeamName {
        /// The team's position in name order, from 1.
        position: usize,
    },
    /// A member is not a person of the deployment's identifier kind.
    #[error(
        "member {position} of the team {team} in the teams document is not a person of this deployment's identifier kind"
    )]
    Member {
        /// The team.
        team: TeamName,
        /// The member's position in the team's list, from 1.
        position: usize,
    },
    /// The document lists more than [`MAX_DIRECTORY_TEAMS`] teams.
    #[error("the teams document lists more than {MAX_DIRECTORY_TEAMS} teams")]
    TooManyTeams,
    /// A team has more than [`MAX_DIRECTORY_MEMBERS`] members.
    #[error("the team {team} in the teams document has more than {MAX_DIRECTORY_MEMBERS} members")]
    TooManyMembers {
        /// The team.
        team: TeamName,
    },
    /// A member belongs to more than [`MAX_TEAMS_JOINED`] teams. `team` is
    /// the first team, in name order, past the limit.
    #[error(
        "member {position} of the team {team} in the teams document belongs to more than {MAX_TEAMS_JOINED} teams"
    )]
    TooManyJoined {
        /// The team.
        team: TeamName,
        /// The member's position in the team's list, from 1.
        position: usize,
    },
}

/// A teams document as JSON reads it. serde_json keeps the last of two
/// keys with one name, so a document that names a team twice keeps the
/// team's last list; a stricter reader would refuse the repeat.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    teams: BTreeMap<String, Vec<String>>,
}

/// The teams of one document: each team's members in identifier order,
/// and each person's teams in name order.
#[derive(Default)]
pub(super) struct Directory {
    teams: BTreeMap<TeamName, Vec<Person>>,
    joined: HashMap<Person, Vec<TeamName>>,
}

impl fmt::Debug for Directory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Directory")
            .field("teams", &self.teams.len())
            .field("people", &self.joined.len())
            .finish()
    }
}

impl Directory {
    /// The teams of `bytes`, whose members are people of `kind`.
    ///
    /// # Errors
    ///
    /// Returns the [`DocumentRefused`] that names the first problem.
    pub(super) fn parse(bytes: &[u8], kind: IdentityKind) -> Result<Self, DocumentRefused> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(DocumentRefused::TooLarge);
        }
        // serde_json's message can quote a value from the document, which
        // may be an address, so only the position is kept.
        let raw: Raw =
            serde_json::from_slice(bytes).map_err(|error| DocumentRefused::Malformed {
                line: error.line(),
                column: error.column(),
            })?;
        if raw.teams.len() > MAX_DIRECTORY_TEAMS {
            return Err(DocumentRefused::TooManyTeams);
        }
        let mut directory = Self::default();
        for (index, (name, listed)) in raw.teams.into_iter().enumerate() {
            let position = index.saturating_add(1);
            let team = TeamName::try_from(name.as_str())
                .map_err(|_| DocumentRefused::TeamName { position })?;
            if listed.len() > MAX_DIRECTORY_MEMBERS {
                return Err(DocumentRefused::TooManyMembers { team });
            }
            let mut members: Vec<Person> = Vec::with_capacity(listed.len());
            let mut seen: HashSet<Person> = HashSet::with_capacity(listed.len());
            for (index, text) in listed.iter().enumerate() {
                let position = index.saturating_add(1);
                let member = kind.person(text).map_err(|_| DocumentRefused::Member {
                    team: team.clone(),
                    position,
                })?;
                if !seen.insert(member.clone()) {
                    continue;
                }
                let teams = directory.joined.entry(member.clone()).or_default();
                if teams.len() >= MAX_TEAMS_JOINED {
                    return Err(DocumentRefused::TooManyJoined { team, position });
                }
                teams.push(team.clone());
                members.push(member);
            }
            members.sort_by(|a, b| a.as_str().cmp(b.as_str()));
            directory.teams.insert(team, members);
        }
        Ok(directory)
    }

    /// The names of the teams that `person` belongs to, in name order.
    pub(super) fn teams_of(&self, person: &Person) -> Vec<TeamName> {
        self.joined.get(person).cloned().unwrap_or_default()
    }

    /// Whether a team has the name.
    pub(super) fn exists(&self, team: &TeamName) -> bool {
        self.teams.contains_key(team)
    }

    /// The members of the team, or `None` when no team has the name.
    pub(super) fn members(&self, team: &TeamName) -> Option<&[Person]> {
        self.teams.get(team).map(Vec::as_slice)
    }
}

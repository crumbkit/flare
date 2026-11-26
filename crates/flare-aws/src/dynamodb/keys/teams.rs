//! The keys of teams and their member pairs.

use super::Key;
use flare_core::{Person, TeamName};

/// The sort key prefix of a member under a team's partition.
pub(crate) const MEMBER_PREFIX: &str = "M#";

/// The sort key prefix of a team under a person's partition.
pub(crate) const TEAM_PREFIX: &str = "T#";

impl Key {
    /// A team's item, which holds its owner, creation time, state and
    /// member count.
    pub(crate) fn team(team: &TeamName) -> Self {
        Self::new(team_partition(team), "META")
    }

    /// The two items of one member pair: the member under the team, and
    /// the team under the member.
    pub(crate) fn member_pair(team: &TeamName, member: &Person) -> (Self, Self) {
        (
            Self::new(
                team_partition(team),
                format!("{MEMBER_PREFIX}{}", member.as_str()),
            ),
            Self::new(
                member_partition(member),
                format!("{TEAM_PREFIX}{}", team.as_str()),
            ),
        )
    }
}

/// The partition of a team's item, its members and its grant entries.
pub(crate) fn team_partition(team: &TeamName) -> String {
    format!("TEAM#{}", team.as_str())
}

/// The partition of a person's teams.
pub(crate) fn member_partition(member: &Person) -> String {
    format!("MEMBER#{}", member.as_str())
}

/// The member of a member entry's sort key, or `None` for any other.
pub(crate) fn member_of(sort_key: &str) -> Option<Person> {
    Person::try_from(sort_key.strip_prefix(MEMBER_PREFIX)?).ok()
}

/// The team of a person's team entry's sort key, or `None` for any other.
pub(crate) fn team_of(sort_key: &str) -> Option<TeamName> {
    TeamName::try_from(sort_key.strip_prefix(TEAM_PREFIX)?).ok()
}

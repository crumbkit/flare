//! The failures of `flare serve` that the teams `hosted.teams` names stop
//! before it binds: a teams document it cannot use, and a table whose team
//! grants were made under other teams.

use super::spelled::{CONFIG_TEAMS_CHANGED, TEAMS_DOCUMENT_REFUSED};
use crumbkit::{Call, Failure, Recovery};

/// Why the teams that `hosted.teams` names stop `flare serve`.
#[derive(Debug, thiserror::Error, Failure)]
pub enum TeamsFailed {
    /// The teams document that `hosted.teams` names could not be read or
    /// was refused, so `flare serve` binds nothing.
    #[error("{problem}")]
    #[failure(
        code = TEAMS_DOCUMENT_REFUSED,
        hint = "fix the teams document that hosted.teams names, or the sync job that writes it"
    )]
    DocumentRefused {
        /// Why, naming hosted.teams and never a member.
        problem: String,
    },
    /// The share API's table holds a team grant made under other teams
    /// than `hosted.teams` names, so `flare serve` wrote nothing and binds
    /// nothing. Under the configured teams the grant would open to
    /// whichever team of its name they hold.
    #[error(
        "the share API's table holds team grants made under {recorded} teams, such as the grant to team:{team} on the share {share}, and hosted.teams names {configured} teams"
    )]
    #[failure(
        code = CONFIG_TEAMS_CHANGED,
        recovery = revoke(share, team),
        alternative = Recovery::hint("set hosted.teams back to the teams the table records"),
        field(recorded = recorded)
    )]
    Changed {
        /// The teams the table records, `stored` or `directory`.
        recorded: String,
        /// The teams that `hosted.teams` names.
        configured: String,
        /// The share of the team grant that was found.
        share: String,
        /// That grant's team.
        team: String,
    },
}

/// The recovery of `config::teams_changed`: the share's owner revokes the
/// team grant that was found, and `flare serve` then names the next one or
/// starts.
fn revoke(share: &str, team: &str) -> Recovery {
    Recovery::Run(
        Call::new(&["revoke"])
            .value("id", share.to_owned())
            .value("with", format!("team:{team}"))
            .flag("yes"),
    )
}

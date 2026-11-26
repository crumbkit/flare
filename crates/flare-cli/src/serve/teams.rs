//! The hosted site's teams: flare's own, in the stores' table, or the
//! deployment's teams document, from a file or from an object in the stores
//! bucket, as `hosted.teams` says. The document is read before the site
//! binds, and a document refused then stops `flare serve`. A document
//! refused on a later read keeps the last good copy and logs
//! `teams.document.refused` with the reason.

use crate::codes::Failed;
use crate::config::TeamsSource;
use flare_aws::{DynamoTeamStore, S3TeamsDocument, Stores};
use flare_core::{
    DocumentRefused, DocumentTeams, DocumentUnreadable, FlareTeams, IdentityKind,
    MAX_DOCUMENT_BYTES, Person, Team, TeamName, TeamView, Teams, TeamsDocument, TeamsError,
    Timestamp,
};
use std::io::Read as _;
use std::path::{Path, PathBuf};

/// The teams document at an absolute path.
#[derive(Debug, Clone)]
pub(crate) struct FileDocument(PathBuf);

impl TeamsDocument for FileDocument {
    async fn read(&self) -> Result<Vec<u8>, DocumentUnreadable> {
        let path = self.0.clone();
        let shown = path.display().to_string();
        #[expect(
            clippy::disallowed_methods,
            reason = "the read awaits its thread on every path, and the thread ends after reading at most one byte past the document limit, also when a dropped call leaves it to finish"
        )]
        let read = tokio::task::spawn_blocking(move || {
            let file = std::fs::File::open(&path)?;
            let mut bytes = Vec::new();
            let most = u64::try_from(MAX_DOCUMENT_BYTES).unwrap_or(u64::MAX);
            file.take(most.saturating_add(1)).read_to_end(&mut bytes)?;
            Ok::<_, std::io::Error>(bytes)
        })
        .await;
        match read {
            Ok(Ok(bytes)) => Ok(bytes),
            Ok(Err(error)) => Err(DocumentUnreadable::new(format!("{shown}: {error}"))),
            Err(_) => Err(DocumentUnreadable::new(format!(
                "{shown}: the read stopped before it finished"
            ))),
        }
    }
}

/// The teams that `hosted.teams` selects.
#[derive(Debug)]
pub(super) enum HostedTeams {
    /// flare's own teams, in the stores' table.
    Stored(FlareTeams<DynamoTeamStore>),
    /// The teams document at a path.
    File(DocumentTeams<FileDocument>),
    /// The teams document in the stores bucket.
    Object(DocumentTeams<S3TeamsDocument>),
}

/// Logs a later read of the document that was refused.
fn report(refusal: &DocumentRefused) {
    tracing::warn!(
        event = "teams.document.refused",
        reason = %refusal,
        "flare keeps the last good copy of the teams document that hosted.teams names"
    );
}

/// The failure that stops `flare serve` when the document is refused.
fn refused(refusal: &DocumentRefused) -> Failed {
    Failed::TeamsDocumentRefused {
        problem: format!("the teams document that hosted.teams names was refused: {refusal}"),
    }
}

/// The teams of the document at `path`, read once now.
///
/// # Errors
///
/// Returns [`Failed::TeamsDocumentRefused`] when the document cannot be
/// read or is refused.
pub(super) async fn file_teams(
    path: &Path,
    kind: IdentityKind,
) -> Result<DocumentTeams<FileDocument>, Failed> {
    let teams = DocumentTeams::load(FileDocument(path.to_path_buf()), kind)
        .await
        .map_err(|refusal| refused(&refusal))?;
    Ok(teams.on_refused(report))
}

/// The teams that `source` selects, with the document read once now.
///
/// # Errors
///
/// Returns [`Failed::TeamsDocumentRefused`] when the document cannot be
/// read or is refused.
pub(super) async fn hosted_teams(
    source: &TeamsSource,
    stores: &Stores,
    kind: IdentityKind,
) -> Result<HostedTeams, Failed> {
    match source {
        TeamsSource::Stored => Ok(HostedTeams::Stored(FlareTeams::new(stores.teams.clone()))),
        TeamsSource::File(path) => Ok(HostedTeams::File(file_teams(path, kind).await?)),
        TeamsSource::Object(key) => {
            let teams = DocumentTeams::load(stores.teams_document(key.clone()), kind)
                .await
                .map_err(|refusal| refused(&refusal))?;
            Ok(HostedTeams::Object(teams.on_refused(report)))
        }
    }
}

/// Calls `$call` on the source that `$teams` holds.
macro_rules! each {
    ($teams:expr, $source:ident => $call:expr) => {
        match $teams {
            HostedTeams::Stored($source) => $call,
            HostedTeams::File($source) => $call,
            HostedTeams::Object($source) => $call,
        }
    };
}

impl Teams for HostedTeams {
    fn manages(&self) -> bool {
        each!(self, teams => teams.manages())
    }

    async fn teams_of(&self, person: &Person) -> Result<Vec<TeamName>, TeamsError> {
        each!(self, teams => teams.teams_of(person).await)
    }

    async fn exists(&self, team: &TeamName) -> Result<bool, TeamsError> {
        each!(self, teams => teams.exists(team).await)
    }

    async fn taken(&self, team: &TeamName) -> Result<bool, TeamsError> {
        each!(self, teams => teams.taken(team).await)
    }

    async fn active(&self, team: &TeamName) -> Result<Option<Timestamp>, TeamsError> {
        each!(self, teams => teams.active(team).await)
    }

    async fn members(&self, team: &TeamName) -> Result<Vec<Person>, TeamsError> {
        each!(self, teams => teams.members(team).await)
    }

    async fn create(
        &self,
        owner: &Person,
        team: TeamName,
        at: Timestamp,
    ) -> Result<Team, TeamsError> {
        each!(self, teams => teams.create(owner, team, at).await)
    }

    async fn add(
        &self,
        actor: &Person,
        team: &TeamName,
        member: &Person,
    ) -> Result<bool, TeamsError> {
        each!(self, teams => teams.add(actor, team, member).await)
    }

    async fn remove(
        &self,
        actor: &Person,
        team: &TeamName,
        member: &Person,
    ) -> Result<bool, TeamsError> {
        each!(self, teams => teams.remove(actor, team, member).await)
    }

    async fn delete(&self, actor: &Person, team: &TeamName) -> Result<(), TeamsError> {
        each!(self, teams => teams.delete(actor, team).await)
    }

    async fn finish_delete(&self, actor: &Person, team: &TeamName) -> Result<(), TeamsError> {
        each!(self, teams => teams.finish_delete(actor, team).await)
    }

    async fn transfer(
        &self,
        actor: &Person,
        team: &TeamName,
        to: &Person,
    ) -> Result<Team, TeamsError> {
        each!(self, teams => teams.transfer(actor, team, to).await)
    }

    async fn show(
        &self,
        actor: &Person,
        team: &TeamName,
    ) -> Result<(TeamView, Vec<Person>), TeamsError> {
        each!(self, teams => teams.show(actor, team).await)
    }

    async fn list(&self, actor: &Person) -> Result<Vec<TeamView>, TeamsError> {
        each!(self, teams => teams.list(actor).await)
    }
}

#[cfg(test)]
mod tests;

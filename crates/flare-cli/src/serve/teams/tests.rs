use super::*;
use crumbkit::Failure as _;
use flare_core::{Person, TeamName, Teams};
use std::io::Write as _;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flare-teams-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("teams.json")
}

#[expect(
    clippy::disallowed_methods,
    reason = "a test writes the teams document that the deployment's sync job would write"
)]
fn write(path: &Path, bytes: &[u8]) {
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(bytes).unwrap();
}

#[tokio::test]
async fn a_teams_file_gives_the_teams_it_lists() {
    let path = scratch("lists");
    write(&path, br#"{"teams": {"design": ["ana@example.com"]}}"#);
    let teams = file_teams(&path, IdentityKind::Email).await.unwrap();
    let ana = Person::try_from("ana@example.com").unwrap();
    assert_eq!(
        teams.teams_of(&ana).await.unwrap(),
        vec![TeamName::try_from("design").unwrap()]
    );
}

#[tokio::test]
async fn a_missing_teams_file_stops_the_start_naming_hosted_teams_and_the_path() {
    let path = scratch("missing").with_file_name("absent.json");
    let failed = file_teams(&path, IdentityKind::Email).await.unwrap_err();
    let Failed::Teams(TeamsFailed::DocumentRefused { problem }) = &failed else {
        panic!("{failed:?}");
    };
    assert!(problem.contains("hosted.teams"), "{problem}");
    assert!(problem.contains("absent.json"), "{problem}");
    assert_eq!(failed.code().as_str(), "flare::teams::document_refused");
}

#[tokio::test]
async fn a_refused_teams_file_stops_the_start_with_the_reason_and_no_member() {
    let path = scratch("refused");
    write(&path, br#"{"teams": {"design": ["ana"]}}"#);
    let failed = file_teams(&path, IdentityKind::Email).await.unwrap_err();
    let Failed::Teams(TeamsFailed::DocumentRefused { problem }) = &failed else {
        panic!("{failed:?}");
    };
    assert!(problem.contains("member 1 of the team design"), "{problem}");
    assert!(!problem.contains("\"ana\""), "{problem}");
}

#[tokio::test]
async fn a_teams_file_is_read_only_one_byte_past_the_limit() {
    let path = scratch("big");
    write(&path, &vec![b' '; MAX_DOCUMENT_BYTES + 4096]);
    let bytes = FileDocument(path).read().await.unwrap();
    assert_eq!(bytes.len(), MAX_DOCUMENT_BYTES + 1);
}

/// A document whose read never finishes, such as a file on a hung mount.
struct Hung;

impl TeamsDocument for Hung {
    async fn read(&self) -> Result<Vec<u8>, DocumentUnreadable> {
        std::future::pending().await
    }
}

#[tokio::test]
async fn a_read_that_does_not_finish_in_time_is_refused_as_unreadable() {
    let timed = Timed::new(Hung, std::time::Duration::from_millis(20));
    let refusal = DocumentTeams::load(timed, IdentityKind::Email)
        .await
        .unwrap_err();
    let DocumentRefused::Unreadable(unreadable) = &refusal else {
        panic!("{refusal:?}");
    };
    assert!(
        unreadable.reason().contains("did not finish within"),
        "{unreadable}"
    );
}

#[tokio::test]
async fn a_read_that_finishes_in_time_gives_the_document() {
    let path = scratch("timed");
    write(&path, br#"{"teams": {"design": ["ana@example.com"]}}"#);
    let timed = Timed::new(FileDocument(path), READ_TIMEOUT);
    let bytes = timed.read().await.unwrap();
    assert!(bytes.starts_with(b"{\"teams\""));
}

#[test]
fn a_file_or_object_source_records_directory_teams_and_stored_records_stored() {
    assert_eq!(teams_kind(&TeamsSource::Stored), TeamsKind::Stored);
    let file = TeamsSource::File(PathBuf::from("/srv/teams.json"));
    assert_eq!(teams_kind(&file), TeamsKind::Directory);
}

//! `flare serve` against DynamoDB Local and SeaweedFS stand-ins when
//! `hosted.teams` changes: the table records the teams of its first start,
//! a start under the other teams is refused while a team grant remains, so
//! no grant opens to another group of people, and once that grant is
//! revoked the start goes on.
//!
//! The issuer that the config names refuses every connection, so a start
//! that gets past the stores and the teams ends with
//! `flare::oidc::provider_unavailable`. These tests sign nobody in, so they
//! run on every system.
use crate::run::{Home, Ran, write};
use crate::stand_in::{
    ACCESS_KEY_ID, SECRET_ACCESS_KEY, create_bucket, create_table, dynamodb_client, random_name,
    stand_in_url,
};
use aws_sdk_dynamodb::types::AttributeValue;
use tokio::runtime::Runtime;

const SHARE: &str = "0123456789abcdef0123456789abcdef";

/// A table, a bucket, a stub board and a teams document for one test.
struct Site {
    runtime: Runtime,
    home: Home,
    table: String,
    bucket: String,
    board: std::path::PathBuf,
    program: std::path::PathBuf,
    document: std::path::PathBuf,
}

impl Site {
    fn new() -> Self {
        let runtime = Runtime::new().unwrap();
        let (table, bucket) = (random_name("flare-teams"), random_name("flare-teams"));
        runtime.block_on(async {
            create_table(&table, None).await;
            create_bucket(&bucket).await;
        });
        let home = Home::new();
        let board = home.path().join("board-root");
        std::fs::create_dir_all(board.join(".board")).unwrap();
        write(&board.join(".board/board.json"), "{}");
        // `flare serve` only asks the board program whether it has
        // `log --at`, and a failure report that is not a usage failure says
        // so; the share API reads no board.
        let program = home.script(
            "board-stub",
            r#"printf '%s\n' '{"schema":"board/failure/v1","code":"board::not_found","message":"no such path","fields":{}}'
exit 1
"#,
        );
        let document = home.path().join("teams.json");
        write(&document, r#"{"teams": {"design": ["ana@example.com"]}}"#);
        Self {
            runtime,
            home,
            table,
            bucket,
            board,
            program,
            document,
        }
    }

    /// Runs `flare serve` to its end with `teams` as `hosted.teams`.
    fn serve(&self, teams: &str) -> Ran {
        let secret = self.home.path().join("client_secret");
        write(&secret, "secret");
        self.home.write_config(&format!(
            r#"{{"hosted":{{"origin":"https://flare.example.org","listen":"127.0.0.1:0","stores":{{"region":"us-east-1","table":"{table}","bucket":"{bucket}"}},"teams":{teams}}},"oidc":{{"issuer":"https://127.0.0.1:9","client_id":"flare","client_secret_file":"{secret}","admit":{{"emails":[{{"email":"ada@example.com","principal":"ada","reach":"everything"}}]}}}}}}"#,
            table = self.table,
            bucket = self.bucket,
            secret = secret.display(),
        ));
        let board = self.board.display().to_string();
        let program = self.program.display().to_string();
        #[expect(clippy::disallowed_methods, reason = "the tests run the built binary")]
        let output = self
            .home
            .command(&["serve", "--board", &board, "--board-program", &program])
            .env(
                "AWS_ENDPOINT_URL_DYNAMODB",
                stand_in_url("FLARE_TEST_DYNAMODB"),
            )
            .env("AWS_ENDPOINT_URL_S3", stand_in_url("FLARE_TEST_S3"))
            .env("AWS_ACCESS_KEY_ID", ACCESS_KEY_ID)
            .env("AWS_SECRET_ACCESS_KEY", SECRET_ACCESS_KEY)
            .output()
            .unwrap();
        Ran::from(output)
    }

    fn file(&self) -> String {
        format!(r#"{{"file":"{}"}}"#, self.document.display())
    }

    /// Writes a grant to the team `design` on the share [`SHARE`], as a
    /// `flare share --with team:design` would.
    fn grant_to_design(&self) {
        self.runtime.block_on(async {
            dynamodb_client()
                .put_item()
                .table_name(&self.table)
                .item("pk", AttributeValue::S(format!("SHARE#{SHARE}")))
                .item("sk", AttributeValue::S("G#team:design".to_owned()))
                .item("role", AttributeValue::S("reader".to_owned()))
                .send()
                .await
                .unwrap();
        });
    }

    /// Deletes that grant, as `flare revoke --with team:design` would.
    fn revoke_design(&self) {
        self.runtime.block_on(async {
            dynamodb_client()
                .delete_item()
                .table_name(&self.table)
                .key("pk", AttributeValue::S(format!("SHARE#{SHARE}")))
                .key("sk", AttributeValue::S("G#team:design".to_owned()))
                .send()
                .await
                .unwrap();
        });
    }
}

/// Asserts that the start got past the stores and the teams.
fn past_the_teams(ran: &Ran) {
    ran.assert_failed("flare::oidc::provider_unavailable");
}

/// Asserts that the start was refused for the teams, naming the grant and
/// the command that revokes it.
fn refused_for_the_teams(ran: &Ran) {
    ran.assert_failed("flare::config::teams_changed");
    let all = ran.all();
    assert!(all.contains("team:design"), "{all}");
    assert!(all.contains(SHARE), "{all}");
    assert!(all.contains("flare revoke"), "{all}");
}

#[test]
fn a_directory_start_on_a_table_whose_stored_teams_hold_a_grant_is_refused_until_it_is_revoked() {
    let site = Site::new();
    past_the_teams(&site.serve(r#""stored""#));
    site.grant_to_design();
    refused_for_the_teams(&site.serve(&site.file()));
    past_the_teams(&site.serve(r#""stored""#));
    site.revoke_design();
    past_the_teams(&site.serve(&site.file()));
}

#[test]
fn a_stored_start_on_a_table_whose_directory_teams_hold_a_grant_is_refused_until_it_is_revoked() {
    let site = Site::new();
    past_the_teams(&site.serve(&site.file()));
    site.grant_to_design();
    refused_for_the_teams(&site.serve(r#""stored""#));
    past_the_teams(&site.serve(&site.file()));
    site.revoke_design();
    past_the_teams(&site.serve(r#""stored""#));
}

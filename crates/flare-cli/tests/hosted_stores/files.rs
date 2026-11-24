//! `flare files sweep` against DynamoDB Local and SeaweedFS stand-ins: it
//! reads the stores from the same config as `flare serve`, lists the files
//! in the bucket and reports what it examined and deleted. The rule of
//! which files it deletes is flare-core's `sweep_files`, whose own tests
//! age files with a chosen clock; a file these tests store is always
//! younger than 30 minutes, so the sweep keeps it.
//!
//! These tests sign nobody in, so they run on every system.
use crate::run::{Home, Ran, write};
use crate::stand_in::{
    ACCESS_KEY_ID, SECRET_ACCESS_KEY, create_bucket, create_table, random_name, s3_client,
    stand_in_url,
};
use serde_json::Value;
use tokio::runtime::Runtime;

/// A share id and a file id, which no version in the table lists.
const KEY: &str = "files/0123456789abcdef0123456789abcdef/fedcba9876543210fedcba9876543210";

/// A home whose config names `table` and `bucket` as the hosted stores.
fn hosted(table: &str, bucket: &str) -> Home {
    let home = Home::new();
    let secret = home.path().join("client_secret");
    write(&secret, "secret");
    home.write_config(&format!(
        r#"{{"hosted":{{"origin":"https://flare.example.org","listen":"127.0.0.1:8080","stores":{{"region":"us-east-1","table":"{table}","bucket":"{bucket}"}}}},"oidc":{{"issuer":"https://issuer.example.org","client_id":"flare","client_secret_file":"{secret}","admit":{{"emails":[{{"email":"ada@example.com","principal":"ada","reach":"everything"}}]}}}}}}"#,
        secret = secret.display(),
    ));
    home
}

/// Runs `flare files sweep --yes --json` with the stand-ins as the AWS
/// endpoints, which the shipped `Stores::connect` reads from the
/// environment.
fn sweep(home: &Home) -> Ran {
    #[expect(clippy::disallowed_methods, reason = "the tests run the built binary")]
    let output = home
        .command(&["files", "sweep", "--yes", "--json"])
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

#[test]
fn flare_files_sweep_examines_the_stored_files_and_keeps_one_stored_less_than_30_minutes_ago() {
    let runtime = Runtime::new().unwrap();
    let (table, bucket) = (random_name("flare-sweep"), random_name("flare-sweep"));
    runtime.block_on(async {
        create_table(&table, None).await;
        create_bucket(&bucket).await;
        s3_client()
            .put_object()
            .bucket(&bucket)
            .key(KEY)
            .content_type("image/png")
            .body(b"PNG".to_vec().into())
            .send()
            .await
            .unwrap();
    });

    let ran = sweep(&hosted(&table, &bucket));
    ran.assert_ok();
    let report: Value = serde_json::from_str(ran.stdout.trim()).unwrap();
    assert_eq!(report["data"]["examined"], 1, "{report}");
    assert_eq!(report["data"]["deleted"], 0, "{report}");
    assert_eq!(report["data"]["undecided"], 0, "{report}");
    assert_eq!(report["data"]["stopped_early"], false, "{report}");
    runtime.block_on(async {
        s3_client()
            .head_object()
            .bucket(&bucket)
            .key(KEY)
            .send()
            .await
            .expect("the sweep keeps a file stored less than 30 minutes ago");
    });
}

#[test]
fn flare_files_sweep_refuses_a_config_that_names_no_hosted_stores() {
    let home = Home::new();
    home.write_config("{}");
    sweep(&home).assert_failed("flare::config::invalid");
}

#[test]
fn flare_files_sweep_refuses_when_the_bucket_does_not_exist() {
    let runtime = Runtime::new().unwrap();
    let table = random_name("flare-sweep");
    runtime.block_on(create_table(&table, None));
    sweep(&hosted(&table, &random_name("flare-sweep"))).assert_failed("flare::stores::unavailable");
}

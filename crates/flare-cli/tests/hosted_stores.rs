//! The built `flare serve` with a hosted config whose `stores` name a table
//! and a bucket, run against DynamoDB Local and SeaweedFS stand-ins: a
//! person signs in through `flare login`, shares a file with `flare share`,
//! and reads it back over the API and through `flare shares`. Another test
//! shares a Markdown file with an image and reads the image back from the
//! file route. The tests read the table and the bucket directly to show
//! that the stores were reached.
//!
//! The flare command's build holds no `testing` feature. The stand-ins are
//! reached through the AWS SDK's own environment variables, which the
//! shipped `Stores::connect` reads, and the fake sign-in provider serves
//! TLS with a certificate authority that the test generates and gives to
//! the children in `SSL_CERT_FILE`. rustls-platform-verifier reads that
//! variable on Linux only, so the tests that sign in run on Linux and pass
//! with a message elsewhere. The tests of `flare files sweep`, in
//! `hosted_stores/files.rs`, sign nobody in and run on every system.
//!
//! The tests need `FLARE_TEST_DYNAMODB` and `FLARE_TEST_S3`, as flare-aws's
//! tests do, and fail naming the one that is unset.
#![cfg(test)]

#[cfg(target_os = "linux")]
#[path = "support/api.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the fake Keychain is test support, and this test uses part of it"
)]
pub mod api;
#[cfg(target_os = "linux")]
#[path = "support/hosted.rs"]
#[expect(
    clippy::disallowed_types,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the hosted rig is test support, and it owns the server and login children it starts and kills them on drop"
)]
pub mod hosted;
#[path = "../../flare-auth-loopback/tests/support/scratch.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the scratch directory is flare-auth-loopback's test support, and these tests use part of it"
)]
pub mod loopback_scratch;
#[path = "support/run.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default,
    reason = "the run helper is test support, and each test file uses part of it"
)]
pub mod run;
#[path = "../../flare-aws/tests/support/stand_in_raw.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the stand-in support is flare-aws's, and this test uses part of it"
)]
pub mod stand_in;
#[cfg(target_os = "linux")]
#[path = "support/tls_provider.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the TLS provider is test support"
)]
pub mod tls_provider;

#[path = "hosted_stores/files.rs"]
mod files;

#[cfg(target_os = "linux")]
mod linux {
    use crate::api::Fakes as _;
    use crate::hosted::{Made, Rig};
    use crate::run::write;
    use crate::stand_in::{dynamodb_client, s3_client};
    use crate::tls_provider::EMAIL;
    use reqwest::Client;
    use serde_json::Value;

    const TEXT: &str = "# Review\n\nThe hosted stores hold this text.\n";

    #[test]
    fn flare_serve_shares_and_reads_back_through_the_table_and_the_bucket_its_config_names() {
        let rig = Rig::new(Made::Both);
        let _server = rig.serve();

        let login = rig.login();
        login.assert_ok();
        assert!(login.stdout.contains(EMAIL), "{}", login.all());
        let item = rig.home.keychain_item().expect("the login saved a token");
        let (origin, token) = item.split_once(' ').unwrap();
        assert_eq!(origin, rig.origin());

        let file = rig.home.path().join("review.md");
        write(&file, TEXT);
        let shared = rig.home.sharing(&[
            "share",
            file.to_str().unwrap(),
            "--with",
            "bob@example.com",
            "--json",
        ]);
        shared.assert_ok();
        let shared: Value = serde_json::from_str(shared.stdout.trim()).unwrap();
        let id = shared["data"]["id"]
            .as_str()
            .expect("a share id")
            .to_owned();
        assert_eq!(shared["data"]["granted"][0], "bob@example.com", "{shared}");

        let opened: Value = rig.runtime.block_on(async {
            let response = rig
                .get(
                    &Client::new(),
                    &format!("/api/shares/{id}"),
                    Some(("authorization", &format!("Bearer {token}"))),
                )
                .await
                .unwrap();
            assert!(response.status().is_success(), "{response:?}");
            serde_json::from_str(&response.text().await.unwrap()).unwrap()
        });
        assert_eq!(opened["content"], TEXT, "{opened}");

        let listed = rig.home.sharing(&["shares", "--json"]);
        listed.assert_ok();
        assert!(listed.stdout.contains(&id), "{}", listed.all());

        // The table holds the token, the share and its grants, and the
        // shared text. A share without images keeps no file in the bucket.
        rig.runtime.block_on(async {
            let table = dynamodb_client()
                .scan()
                .table_name(&rig.table)
                .send()
                .await
                .unwrap();
            assert!(table.count() >= 3, "{:?}", table.items());
            assert!(
                format!("{:?}", table.items()).contains("The hosted stores hold this text."),
                "{:?}",
                table.items()
            );
        });
    }

    /// A 1-by-1 PNG, which the server decodes.
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    #[test]
    fn flare_serve_keeps_a_shared_documents_image_in_the_bucket_its_config_names_and_serves_it() {
        use base64::Engine as _;
        use std::io::Write as _;

        let rig = Rig::new(Made::Both);
        let _server = rig.serve();
        rig.login().assert_ok();
        let item = rig.home.keychain_item().expect("the login saved a token");
        let (_, token) = item.split_once(' ').unwrap();

        let file = rig.home.path().join("chart.md");
        write(&file, "# Chart\n\n![chart](chart.png)\n");
        let png = base64::engine::general_purpose::STANDARD
            .decode(PNG)
            .unwrap();
        #[expect(
            clippy::disallowed_methods,
            reason = "a test writes its fixture files directly"
        )]
        let mut image = std::fs::File::create(rig.home.path().join("chart.png")).unwrap();
        image.write_all(&png).unwrap();
        let shared = rig
            .home
            .sharing(&["share", file.to_str().unwrap(), "--json"]);
        shared.assert_ok();
        let shared: Value = serde_json::from_str(shared.stdout.trim()).unwrap();
        let id = shared["data"]["id"]
            .as_str()
            .expect("a share id")
            .to_owned();

        rig.runtime.block_on(async {
            let listed = s3_client()
                .list_objects_v2()
                .bucket(&rig.bucket)
                .prefix(format!("files/{id}/"))
                .send()
                .await
                .unwrap();
            let keys: Vec<&str> = listed
                .contents()
                .iter()
                .filter_map(|object| object.key())
                .collect();
            assert_eq!(keys.len(), 1, "{keys:?}");
            let file_id = keys[0].rsplit('/').next().unwrap();
            let response = rig
                .get(
                    &Client::new(),
                    &format!("/api/shares/{id}/versions/1/files/{file_id}"),
                    Some(("authorization", &format!("Bearer {token}"))),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), 200, "{response:?}");
            assert_eq!(
                response.headers()["content-type"].to_str().unwrap(),
                "image/png"
            );
            assert_eq!(response.bytes().await.unwrap().as_ref(), png.as_slice());
        });
    }

    #[test]
    fn flare_serve_refuses_to_start_when_the_table_does_not_exist() {
        let rig = Rig::new(Made::Neither);
        rig.serve_to_end()
            .assert_failed("flare::stores::unavailable");
    }

    #[test]
    fn flare_serve_refuses_to_start_when_the_bucket_does_not_exist() {
        let rig = Rig::new(Made::TableOnly);
        rig.serve_to_end()
            .assert_failed("flare::stores::unavailable");
    }
}

#[cfg(not(target_os = "linux"))]
#[test]
fn the_hosted_stores_tests_that_sign_in_run_on_linux_only() {
    // rustls-platform-verifier trusts extra roots through SSL_CERT_FILE only
    // on Linux; macOS and Windows read their system stores, and trusting the
    // test's CA there would change the person's machine. CI on Linux carries
    // these tests, and a developer on another system can run them in a Linux
    // container.
    eprintln!(
        "the hosted_stores tests that sign in run on Linux only: the TLS provider is trusted \
             through SSL_CERT_FILE, which rustls-platform-verifier reads on Linux; run them in a \
             Linux container"
    );
}

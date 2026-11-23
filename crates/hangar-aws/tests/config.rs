//! The hosted config parser admits values at their limits and refuses
//! credential keys, unknown keys and malformed names without repeating a
//! value.
#![cfg(test)]

use hangar_aws::{BucketName, ConfigError, HostedConfig, Region, TableName};

fn parse(text: &str) -> Result<HostedConfig, String> {
    serde_json::from_str::<HostedConfig>(text).map_err(|error| error.to_string())
}

fn config(region: &str, table: &str, bucket: &str) -> String {
    serde_json::json!({"region": region, "table": table, "bucket": bucket}).to_string()
}

#[test]
fn a_config_with_three_valid_names_is_admitted() {
    let parsed = parse(&config("eu-west-1", "flare-shares", "flare-files")).unwrap();
    assert_eq!(parsed.region().as_str(), "eu-west-1");
    assert_eq!(parsed.table().as_str(), "flare-shares");
    assert_eq!(parsed.bucket().as_str(), "flare-files");
}

#[test]
fn names_at_their_length_limits_are_admitted() {
    let table = "t".repeat(255);
    let bucket = format!("b{}9", "-".repeat(61));
    let region = "r".repeat(32);
    assert!(TableName::try_from("abc").is_ok());
    assert!(TableName::try_from(table.as_str()).is_ok());
    assert!(BucketName::try_from("abc").is_ok());
    assert!(BucketName::try_from(bucket.as_str()).is_ok());
    assert!(Region::try_from("a").is_ok());
    assert!(Region::try_from(region.as_str()).is_ok());
    assert!(parse(&config(&region, &table, &bucket)).is_ok());
}

#[test]
fn names_past_their_length_limits_are_refused() {
    let table = "t".repeat(256);
    let bucket = "b".repeat(64);
    let region = "r".repeat(33);
    assert!(TableName::try_from("ab").is_err());
    assert!(TableName::try_from(table.as_str()).is_err());
    assert!(BucketName::try_from("ab").is_err());
    assert!(BucketName::try_from(bucket.as_str()).is_err());
    assert!(Region::try_from("").is_err());
    assert!(Region::try_from(region.as_str()).is_err());
}

#[test]
fn malformed_names_are_refused_without_repeating_them() {
    let cases = [
        ("Eu-West-1", "flare-shares", "flare-files"),
        ("eu west 1", "flare-shares", "flare-files"),
        ("eu-west-1", "flare/shares", "flare-files"),
        ("eu-west-1", "flare-shares", "Flare-Files"),
        ("eu-west-1", "flare-shares", "-flare-files"),
        ("eu-west-1", "flare-shares", "flare-files-"),
        ("eu-west-1", "flare-shares", "flare_files"),
    ];
    for (region, table, bucket) in cases {
        let message = parse(&config(region, table, bucket)).unwrap_err();
        for value in [region, table, bucket] {
            assert!(
                !message.contains(value),
                "the refusal {message:?} repeats a value"
            );
        }
    }
}

#[test]
fn a_credential_key_is_refused_with_a_message_that_names_the_provider_chain() {
    for key in [
        "access_key_id",
        "secret_access_key",
        "session_token",
        "credentials",
        "profile",
    ] {
        let text = serde_json::json!({
            "region": "eu-west-1",
            "table": "flare-shares",
            "bucket": "flare-files",
            key: "AKIASECRETVALUE1234",
        })
        .to_string();
        let message = parse(&text).unwrap_err();
        assert!(message.contains("provider chain"), "{message}");
        assert!(!message.contains("AKIASECRETVALUE1234"), "{message}");
    }
}

#[test]
fn an_unknown_key_is_refused_without_repeating_its_value() {
    let text = serde_json::json!({
        "region": "eu-west-1",
        "table": "flare-shares",
        "bucket": "flare-files",
        "endpoint": "https://example.org/private-value",
    })
    .to_string();
    let message = parse(&text).unwrap_err();
    assert!(message.contains("unknown key"), "{message}");
    assert!(!message.contains("private-value"), "{message}");
}

#[test]
fn a_value_of_the_wrong_type_is_refused_without_repeating_it() {
    let text = r#"{"region": 4242, "table": "flare-shares", "bucket": "flare-files"}"#;
    let message = parse(text).unwrap_err();
    assert!(!message.contains("4242"), "{message}");
    let text = r#"{"region": "eu-west-1", "table": ["secret-list"], "bucket": "flare-files"}"#;
    let message = parse(text).unwrap_err();
    assert!(!message.contains("secret-list"), "{message}");
}

#[test]
fn a_missing_key_is_named() {
    let message = parse(r#"{"region": "eu-west-1", "table": "flare-shares"}"#).unwrap_err();
    assert!(message.contains("bucket"), "{message}");
}

#[test]
fn a_repeated_key_is_refused() {
    let text = r#"{"region": "eu-west-1", "region": "eu-west-2", "table": "t-1", "bucket": "b-1"}"#;
    assert!(parse(text).is_err());
}

#[test]
fn the_error_type_names_no_value_in_its_debug_output() {
    let error = Region::try_from("Secret-Region").unwrap_err();
    assert!(matches!(error, ConfigError::Malformed { .. }));
    assert!(!format!("{error:?} {error}").contains("Secret-Region"));
}

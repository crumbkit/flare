//! The part of the stand-in support that imports nothing from flare-aws's
//! `testing` feature: the stand-ins' URLs, the made-up keys they accept,
//! clients for them, and the creation of a table and a bucket. flare-aws's
//! tests reach it through `stand_in.rs`, and flare-cli's `hosted_stores`
//! test includes it with `#[path]`, so the flare command's dependency tree
//! needs no testing feature.

use aws_sdk_dynamodb::types::{
    AttributeDefinition, BillingMode, KeySchemaElement, KeyType, ScalarAttributeType,
};

/// The made-up access key id that the stand-ins accept.
pub const ACCESS_KEY_ID: &str = "flaretest";
/// The made-up secret key that the stand-ins accept.
pub const SECRET_ACCESS_KEY: &str = "flaretestsecret";

/// The attribute that the table's TTL reads, as the template names it.
pub const TTL_ATTRIBUTE: &str = "expires_at";

/// The CloudFormation template that defines the table and the bucket.
pub const TEMPLATE: &str = include_str!("../../../../infra/flare-stores.template.json");

/// Reads one stand-in's URL from the environment.
#[must_use]
pub fn stand_in_url(variable: &str) -> String {
    #[expect(
        clippy::disallowed_methods,
        reason = "a test reads its own settings from the environment"
    )]
    let value = std::env::var(variable);
    value.unwrap_or_else(|_| {
        panic!(
            "these tests need {variable}: start the stand-ins as \
             crates/flare-aws/README.md says and set {variable} to the URL"
        )
    })
}

/// A random lower-case name with `prefix`.
#[must_use]
pub fn random_name(prefix: &str) -> String {
    let mut bytes = [0_u8; 8];
    aws_lc_rs::rand::fill(&mut bytes).unwrap();
    let suffix: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{prefix}-{suffix}")
}

/// A DynamoDB client for the stand-in, which only test support builds.
#[expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "test support creates and inspects tables on the stand-in directly"
)]
#[must_use]
pub fn dynamodb_client() -> aws_sdk_dynamodb::Client {
    let config = aws_sdk_dynamodb::Config::builder()
        .behavior_version(aws_sdk_dynamodb::config::BehaviorVersion::latest())
        .region(aws_sdk_dynamodb::config::Region::new("us-east-1"))
        .endpoint_url(stand_in_url("FLARE_TEST_DYNAMODB"))
        .credentials_provider(aws_sdk_dynamodb::config::Credentials::new(
            ACCESS_KEY_ID,
            SECRET_ACCESS_KEY,
            None,
            None,
            "flare-tests",
        ))
        .build();
    aws_sdk_dynamodb::Client::from_conf(config)
}

/// An S3 client for the stand-in, which only test support builds.
#[expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "test support creates buckets and writes foreign objects on the stand-in directly"
)]
#[must_use]
pub fn s3_client() -> aws_sdk_s3::Client {
    let config = aws_sdk_s3::Config::builder()
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .endpoint_url(stand_in_url("FLARE_TEST_S3"))
        .force_path_style(true)
        .credentials_provider(aws_sdk_s3::config::Credentials::new(
            ACCESS_KEY_ID,
            SECRET_ACCESS_KEY,
            None,
            None,
            "flare-tests",
        ))
        .build();
    aws_sdk_s3::Client::from_conf(config)
}

/// The table's key schema and attribute definitions, read from the
/// template.
fn template_keys() -> (Vec<KeySchemaElement>, Vec<AttributeDefinition>) {
    let template: serde_json::Value = serde_json::from_str(TEMPLATE).unwrap();
    let table = &template["Resources"]["SharesTable"]["Properties"];
    let schema = table["KeySchema"]
        .as_array()
        .unwrap()
        .iter()
        .map(|element| {
            KeySchemaElement::builder()
                .attribute_name(element["AttributeName"].as_str().unwrap())
                .key_type(KeyType::from(element["KeyType"].as_str().unwrap()))
                .build()
                .unwrap()
        })
        .collect();
    let definitions = table["AttributeDefinitions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|definition| {
            AttributeDefinition::builder()
                .attribute_name(definition["AttributeName"].as_str().unwrap())
                .attribute_type(ScalarAttributeType::from(
                    definition["AttributeType"].as_str().unwrap(),
                ))
                .build()
                .unwrap()
        })
        .collect();
    (schema, definitions)
}

/// Creates a table with the template's keys, or with `keys` when given.
pub async fn create_table(
    name: &str,
    keys: Option<(Vec<KeySchemaElement>, Vec<AttributeDefinition>)>,
) {
    let (schema, definitions) = keys.unwrap_or_else(template_keys);
    dynamodb_client()
        .create_table()
        .table_name(name)
        .set_key_schema(Some(schema))
        .set_attribute_definitions(Some(definitions))
        .billing_mode(BillingMode::PayPerRequest)
        .send()
        .await
        .unwrap();
}

/// Creates a bucket.
pub async fn create_bucket(name: &str) {
    s3_client()
        .create_bucket()
        .bucket(name)
        .send()
        .await
        .unwrap();
}

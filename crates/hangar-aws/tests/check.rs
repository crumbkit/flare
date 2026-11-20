//! `Stores::check` passes for a table and a bucket that the template
//! defines, and names the missing or mismatched part otherwise, so a server
//! can refuse to start.
#![cfg(test)]

#[path = "support/stand_in.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the stand-in support is test support: it panics on a broken fixture"
)]
pub mod stand_in;

use aws_sdk_dynamodb::types::{
    AttributeDefinition, KeySchemaElement, KeyType, ScalarAttributeType,
};
use hangar_aws::{CheckError, Stores};
use stand_in::{create_bucket, create_table, endpoints, fresh_config, unmade_config};

#[tokio::test(flavor = "multi_thread")]
async fn the_check_passes_for_a_table_and_a_bucket_the_template_defines() {
    let stores = Stores::connect_at(&fresh_config().await, &endpoints());
    assert_eq!(stores.check().await, Ok(()));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_check_fails_for_a_table_with_another_key_schema() {
    let config = unmade_config();
    let schema = vec![
        KeySchemaElement::builder()
            .attribute_name("id")
            .key_type(KeyType::Hash)
            .build()
            .unwrap(),
    ];
    let definitions = vec![
        AttributeDefinition::builder()
            .attribute_name("id")
            .attribute_type(ScalarAttributeType::S)
            .build()
            .unwrap(),
    ];
    create_table(config.table().as_str(), Some((schema, definitions))).await;
    create_bucket(config.bucket().as_str()).await;
    let stores = Stores::connect_at(&config, &endpoints());
    assert_eq!(stores.check().await, Err(CheckError::TableKeySchema));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_check_fails_for_a_missing_table() {
    let config = unmade_config();
    create_bucket(config.bucket().as_str()).await;
    let stores = Stores::connect_at(&config, &endpoints());
    assert_eq!(stores.check().await, Err(CheckError::TableMissing));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_check_fails_for_a_missing_bucket() {
    let config = unmade_config();
    create_table(config.table().as_str(), None).await;
    let stores = Stores::connect_at(&config, &endpoints());
    assert_eq!(stores.check().await, Err(CheckError::BucketMissing));
}

#[test]
fn each_check_failure_names_the_part_to_fix() {
    assert!(CheckError::TableMissing.to_string().contains("table"));
    assert!(
        CheckError::TableKeySchema
            .to_string()
            .contains("key schema")
    );
    assert!(CheckError::BucketMissing.to_string().contains("bucket"));
}

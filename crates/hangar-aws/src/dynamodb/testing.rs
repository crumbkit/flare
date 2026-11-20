//! Tables for the crate's own tests: one on the DynamoDB stand-in, created
//! with a random name and the template's keys, and one that no request can
//! reach.

use crate::dynamodb::retry::Policy;
use crate::dynamodb::{DynamoClient, Faults, Table};
use aws_sdk_dynamodb::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_dynamodb::types::{
    AttributeDefinition, BillingMode, KeySchemaElement, KeyType, ScalarAttributeType,
};

#[expect(
    clippy::disallowed_methods,
    reason = "dynamodb.rs owns the DynamoDB client (SVC-12), and its tests build one for the stand-in"
)]
fn client(endpoint: &str) -> DynamoClient {
    let config = aws_sdk_dynamodb::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .endpoint_url(endpoint)
        .credentials_provider(Credentials::new(
            "hangartest",
            "hangartestsecret",
            None,
            None,
            "hangar-tests",
        ))
        .build();
    DynamoClient::from_conf(config)
}

impl Table {
    /// A new table on the stand-in that `HANGAR_TEST_DYNAMODB` names.
    ///
    /// # Panics
    ///
    /// Panics when the variable is unset or the stand-in refuses the table.
    pub(crate) async fn fresh_on_stand_in() -> Self {
        #[expect(
            clippy::disallowed_methods,
            reason = "a test reads its own settings from the environment"
        )]
        let endpoint = std::env::var("HANGAR_TEST_DYNAMODB").expect(
            "the hangar-aws tests need HANGAR_TEST_DYNAMODB: start the stand-ins as \
             crates/hangar-aws/README.md says and set it to DynamoDB Local's URL",
        );
        let client = client(&endpoint);
        let name = format!("hangar-unit-{}", crate::dynamodb::random_hex().unwrap());
        let key = |name: &str, key_type| {
            KeySchemaElement::builder()
                .attribute_name(name)
                .key_type(key_type)
                .build()
                .unwrap()
        };
        let text = |name: &str| {
            AttributeDefinition::builder()
                .attribute_name(name)
                .attribute_type(ScalarAttributeType::S)
                .build()
                .unwrap()
        };
        client
            .create_table()
            .table_name(&name)
            .key_schema(key("pk", KeyType::Hash))
            .key_schema(key("sk", KeyType::Range))
            .attribute_definitions(text("pk"))
            .attribute_definitions(text("sk"))
            .billing_mode(BillingMode::PayPerRequest)
            .send()
            .await
            .unwrap();
        Self::new(client, &name, Policy::STORE, Faults::None)
    }

    /// A table on a port that nothing listens on, with a short deadline.
    pub(crate) fn unreachable() -> Self {
        let policy = Policy {
            deadline: std::time::Duration::from_millis(200),
            ..Policy::STORE
        };
        Self::new(
            client("http://127.0.0.1:9"),
            "hangar-unreachable",
            policy,
            Faults::None,
        )
    }
}

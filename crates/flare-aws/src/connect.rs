//! [`Stores`]: the five stores on one table and one bucket, built from a
//! [`HostedConfig`] and the AWS provider chain, and the check that a server
//! makes before it listens.

use crate::config::HostedConfig;
use crate::dynamodb::retry::Policy;
use crate::dynamodb::{
    DynamoClient, DynamoCommentStore, DynamoCursorStore, DynamoGrantStore, DynamoResponseStore,
    DynamoSealingKeys, DynamoSessionStore, DynamoShareStore, DynamoTeamStore, DynamoTokenStore,
    Faults, Table,
};
use crate::s3::{Bucket, S3Client, S3FileStore};
use aws_sdk_dynamodb::types::{KeyType, ScalarAttributeType};
use std::time::Duration;

/// How long one attempt of one call may take before the SDK gives up on it.
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(6);

/// The stores of a hosted flare.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Stores {
    /// Shares and their versions, in the table.
    pub shares: DynamoShareStore,
    /// The grants of shares, in the table.
    pub grants: DynamoGrantStore,
    /// The files of shares, in the bucket.
    pub files: S3FileStore,
    /// Login codes and CLI tokens, in the table.
    pub tokens: DynamoTokenStore,
    /// The links from responses to flares, in the table.
    pub responses: DynamoResponseStore,
    /// Teams and their members, in the table.
    pub teams: DynamoTeamStore,
    /// Comments and replies, in the table, which share the responses'
    /// position counter.
    pub comments: DynamoCommentStore,
    /// Named activity cursors, in the table.
    pub cursors: DynamoCursorStore,
    /// Browser sessions, in the table.
    pub sessions: DynamoSessionStore,
    /// The keys that seal the sign-in state, in the table.
    pub keys: DynamoSealingKeys,
    table: Table,
    bucket: Bucket,
}

/// What [`Stores::check`] found wrong. Each names the part to fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CheckError {
    /// The table does not exist.
    #[error("the DynamoDB table does not exist; create it from infra/flare-stores.template.json")]
    TableMissing,
    /// The table's keys are not the string keys `pk` and `sk`.
    #[error(
        "the DynamoDB table has another key schema; flare needs the string hash key pk and the \
         string range key sk that infra/flare-stores.template.json defines"
    )]
    TableKeySchema,
    /// The bucket does not exist.
    #[error("the S3 bucket does not exist; create it from infra/flare-stores.template.json")]
    BucketMissing,
    /// The table or the bucket could not be read, such as for a refused
    /// request or a network failure.
    #[error(
        "the DynamoDB table or the S3 bucket could not be read; check the credentials and the network"
    )]
    Unavailable,
}

impl Stores {
    /// The stores for `config`, with credentials from the AWS provider
    /// chain: the environment, a web identity token, or a container or
    /// instance role.
    #[expect(
        clippy::disallowed_methods,
        reason = "connect.rs builds the DynamoDB and S3 clients (SVC-12)"
    )]
    pub async fn connect(config: &HostedConfig) -> Self {
        let timeouts = aws_config::timeout::TimeoutConfig::builder()
            .operation_attempt_timeout(ATTEMPT_TIMEOUT)
            .operation_timeout(Policy::STORE.deadline)
            .build();
        let shared = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(config.region().as_str().to_owned()))
            .timeout_config(timeouts)
            .load()
            .await;
        Self::from_clients(
            config,
            DynamoClient::new(&shared),
            S3Client::new(&shared),
            Policy::STORE,
            Faults::None,
        )
    }

    fn from_clients(
        config: &HostedConfig,
        dynamodb: DynamoClient,
        s3: S3Client,
        policy: Policy,
        faults: Faults,
    ) -> Self {
        let table = Table::new(dynamodb, config.table().as_str(), policy, faults);
        let bucket = Bucket::new(s3, config.bucket().as_str(), policy, faults);
        Self {
            shares: DynamoShareStore::from_table(table.clone()),
            grants: DynamoGrantStore::from_table(table.clone()),
            files: S3FileStore::from_bucket(bucket.clone()),
            tokens: DynamoTokenStore::from_table(table.clone()),
            teams: DynamoTeamStore::from_table(table.clone()),
            responses: DynamoResponseStore::from_table(table.clone()),
            comments: DynamoCommentStore::from_table(table.clone()),
            cursors: DynamoCursorStore::from_table(table.clone()),
            sessions: DynamoSessionStore::from_table(table.clone()),
            keys: DynamoSealingKeys::from_table(table.clone()),
            table,
            bucket,
        }
    }

    /// Checks that the table exists with the string keys `pk` and `sk`, and
    /// that the bucket exists, so a server can refuse to start.
    ///
    /// # Errors
    ///
    /// Returns the [`CheckError`] that names the missing or mismatched part.
    pub async fn check(&self) -> Result<(), CheckError> {
        let described = self
            .table
            .client()
            .describe_table()
            .table_name(self.table.name())
            .send()
            .await;
        let table = match described {
            Ok(output) => output.table.ok_or(CheckError::TableMissing)?,
            Err(error)
                if error
                    .as_service_error()
                    .is_some_and(|error| error.is_resource_not_found_exception()) =>
            {
                return Err(CheckError::TableMissing);
            }
            Err(_) => return Err(CheckError::Unavailable),
        };
        let keys: Vec<(&str, &KeyType)> = table
            .key_schema()
            .iter()
            .map(|key| (key.attribute_name(), key.key_type()))
            .collect();
        let strings = ["pk", "sk"].iter().all(|name| {
            table.attribute_definitions().iter().any(|definition| {
                definition.attribute_name() == *name
                    && *definition.attribute_type() == ScalarAttributeType::S
            })
        });
        if keys != [("pk", &KeyType::Hash), ("sk", &KeyType::Range)] || !strings {
            return Err(CheckError::TableKeySchema);
        }
        let headed = self
            .bucket
            .client()
            .head_bucket()
            .bucket(self.bucket.name())
            .send()
            .await;
        match headed {
            Ok(_) => Ok(()),
            Err(error)
                if error
                    .raw_response()
                    .is_some_and(|raw| raw.status().as_u16() == 404) =>
            {
                Err(CheckError::BucketMissing)
            }
            Err(_) => Err(CheckError::Unavailable),
        }
    }
}

#[cfg(any(test, feature = "testing"))]
pub use stand_ins::{EndpointUrl, Endpoints, Fault};

/// The stores on local stand-ins, for tests.
#[cfg(any(test, feature = "testing"))]
mod stand_ins;

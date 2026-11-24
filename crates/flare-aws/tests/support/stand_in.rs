//! The local stand-ins that the tests run against, and a new table and
//! bucket for each test.
//!
//! The tests read the stand-ins' URLs from `FLARE_TEST_DYNAMODB` and
//! `FLARE_TEST_S3`, and fail naming the variable when one is unset. Each
//! call to [`fresh`] creates a table and a bucket with random names, so
//! tests that run together share nothing. The table takes its key schema
//! and attribute definitions from the CloudFormation template, so the tests
//! run against the table that the template creates.
//!
//! The stand-ins accept the fixed made-up keys that `Stores::connect_at`
//! signs with; no test reads a real credential.

use flare_aws::{BucketName, EndpointUrl, Endpoints, HostedConfig, Region, Stores, TableName};
use flare_core::contract::RunTogether;
use std::future::Future;

#[path = "stand_in_raw.rs"]
mod raw;

pub use raw::{
    ACCESS_KEY_ID, SECRET_ACCESS_KEY, TEMPLATE, TTL_ATTRIBUTE, create_bucket, create_table,
    dynamodb_client, random_name, s3_client, stand_in_url,
};

/// The stand-ins' endpoints.
#[must_use]
pub fn endpoints() -> Endpoints {
    Endpoints::new(
        EndpointUrl::try_from(stand_in_url("FLARE_TEST_DYNAMODB").as_str()).unwrap(),
        EndpointUrl::try_from(stand_in_url("FLARE_TEST_S3").as_str()).unwrap(),
    )
}

/// A config that names a new table and a new bucket, both created.
pub async fn fresh_config() -> HostedConfig {
    let config = unmade_config();
    create_table(config.table().as_str(), None).await;
    create_bucket(config.bucket().as_str()).await;
    config
}

/// A config with random names whose table and bucket do not exist.
#[must_use]
pub fn unmade_config() -> HostedConfig {
    HostedConfig::new(
        Region::try_from("us-east-1").unwrap(),
        TableName::try_from(random_name("flare-test").as_str()).unwrap(),
        BucketName::try_from(random_name("flare-test").as_str()).unwrap(),
    )
}

/// Stores on a new table and a new bucket.
pub async fn fresh() -> Stores {
    Stores::connect_at(&fresh_config().await, &endpoints())
}

/// The runner for the stores' futures, which need a Tokio runtime: it
/// awaits both calls on the current task, so their requests are in flight
/// together.
#[derive(Debug, Clone, Copy)]
pub struct Joined;

impl RunTogether for Joined {
    async fn run_together<A, B, FA, FB>(
        &self,
        first: impl FnOnce() -> FA + Send,
        second: impl FnOnce() -> FB + Send,
    ) -> Option<(A, B)>
    where
        A: Send,
        B: Send,
        FA: Future<Output = A> + Send,
        FB: Future<Output = B> + Send,
    {
        Some(tokio::join!(first(), second()))
    }
}

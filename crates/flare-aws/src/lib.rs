//! flare's share, grant and file stores, kept in DynamoDB and S3.
//!
//! [`Stores::connect`] builds a [`DynamoShareStore`], a
//! [`DynamoGrantStore`] and an [`S3FileStore`] from a [`HostedConfig`],
//! which names the region, the table and the bucket. Credentials come only
//! from the AWS provider chain. The table and the bucket are those that
//! `infra/flare-stores.template.json` defines, and [`Stores::check`]
//! confirms them before a server listens.
//!
//! Each store implements its flare-core trait and passes that trait's
//! contract suite. A share, its versions and its grants are items of one
//! table with no secondary index, so every read, including both lists, is
//! consistent. Each write is one transaction. A write whose answer is lost
//! is sent again with the same token and then settled by a consistent read,
//! so `Unavailable` from a write means its outcome is unknown.
//!
//! No public item names a type of the AWS SDK. The `testing` feature adds
//! `Stores::connect_at`, which points the stores at local stand-ins.

mod config;
mod connect;
mod dynamodb;
mod s3;

pub use config::{BucketName, ConfigError, HostedConfig, Region, TableName};
pub use connect::{CheckError, Stores};
#[cfg(any(test, feature = "testing"))]
pub use connect::{EndpointUrl, Endpoints, Fault};
pub use dynamodb::{DynamoGrantStore, DynamoShareStore};
pub use s3::S3FileStore;

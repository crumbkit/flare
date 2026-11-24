//! flare's share, grant, file, token, response, team, comment, cursor,
//! session and sealing key stores, kept in DynamoDB and S3.
//!
//! [`Stores::connect`] builds a [`DynamoShareStore`], a
//! [`DynamoGrantStore`], an [`S3FileStore`], a [`DynamoTokenStore`], a
//! [`DynamoResponseStore`], a [`DynamoTeamStore`], a [`DynamoCommentStore`],
//! a [`DynamoCursorStore`], a [`DynamoSessionStore`] and a
//! [`DynamoSealingKeys`] from a [`HostedConfig`], which names the region,
//! the table and the bucket. Credentials come only
//! from the AWS provider chain. The table and the bucket are those that
//! `infra/flare-stores.template.json` defines, and [`Stores::check`]
//! confirms them before a server listens.
//!
//! Each store implements its flare-core trait and passes that trait's
//! contract suite. A share, its versions, its grants, login codes, tokens
//! response links, teams, comments, cursors, browser sessions and sealing keys are items of one table with no secondary index, so
//! every read, including every list, is consistent. Each write is one transaction. A write whose answer is lost
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
pub use dynamodb::{
    DynamoCommentStore, DynamoCursorStore, DynamoGrantStore, DynamoResponseStore,
    DynamoSealingKeys, DynamoSessionStore, DynamoShareStore, DynamoTeamStore, DynamoTokenStore,
};
pub use s3::S3FileStore;

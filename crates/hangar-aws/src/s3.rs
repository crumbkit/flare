//! The S3 file store and the one bucket it uses.
//!
//! [`Bucket`] is the only holder of the S3 client. The bucket holds only
//! `files/<share id>/<file id>`, and the store never replaces or deletes an
//! object.

mod fault;
mod files;
mod keys;

pub use files::S3FileStore;

use crate::dynamodb::Faults;
use crate::dynamodb::retry::Policy;
use std::fmt;
use std::sync::Arc;

#[expect(clippy::disallowed_types, reason = "s3.rs owns the S3 client (SVC-12)")]
pub(crate) type S3Client = aws_sdk_s3::Client;

/// The bucket, its client, the retry policy and the injected fault.
#[derive(Clone)]
pub(crate) struct Bucket(Arc<BucketParts>);

struct BucketParts {
    client: S3Client,
    name: String,
    policy: Policy,
    faults: Faults,
}

impl fmt::Debug for Bucket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Bucket")
            .field("name", &self.0.name)
            .finish_non_exhaustive()
    }
}

impl Bucket {
    /// The bucket `name` through `client`.
    pub(crate) fn new(client: S3Client, name: &str, policy: Policy, faults: Faults) -> Self {
        Self(Arc::new(BucketParts {
            client,
            name: name.to_owned(),
            policy,
            faults,
        }))
    }

    pub(crate) fn client(&self) -> &S3Client {
        &self.0.client
    }

    pub(crate) fn name(&self) -> &str {
        &self.0.name
    }

    pub(crate) fn policy(&self) -> &Policy {
        &self.0.policy
    }

    pub(crate) fn faults(&self) -> Faults {
        self.0.faults
    }

    /// A bucket on a port that nothing listens on.
    #[cfg(test)]
    #[expect(
        clippy::disallowed_methods,
        reason = "s3.rs owns the S3 client (SVC-12), and its tests build one that reaches nothing"
    )]
    pub(crate) fn unreachable() -> Self {
        let config = aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .region(aws_sdk_s3::config::Region::new("us-east-1"))
            .endpoint_url("http://127.0.0.1:9")
            .force_path_style(true)
            .build();
        Self::new(
            S3Client::from_conf(config),
            "hangar-unreachable",
            Policy::STORE,
            Faults::None,
        )
    }
}

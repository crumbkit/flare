//! Stores on local stand-ins for DynamoDB and S3, for tests.
//!
//! The clients sign with the fixed made-up keys `hangartest` and
//! `hangartestsecret`, which a stand-in started as the crate's README says
//! accepts, and read nothing from the environment or the AWS config files.
//! An endpoint is never a key of [`HostedConfig`], so a config file cannot
//! send signed requests to another host.

use super::Stores;
use crate::config::{ConfigError, HostedConfig};
use crate::dynamodb::retry::Policy;
use crate::dynamodb::{DynamoClient, Faults};
use crate::s3::S3Client;
use std::time::Duration;

/// The made-up access key id that the stand-ins accept.
const ACCESS_KEY_ID: &str = "hangartest";
/// The made-up secret key that the stand-ins accept.
const SECRET_ACCESS_KEY: &str = "hangartestsecret";

/// The longest endpoint URL, in bytes.
const MAX_ENDPOINT: usize = 2_048;

/// The URL of a stand-in, such as `http://127.0.0.1:8000`: `http://` or
/// `https://` and then 1 or more visible ASCII characters, at most 2,048
/// bytes in all.
///
/// Shortcut: the type checks the scheme and the characters and leaves the
/// rest of the URL to the SDK, because only tests build one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointUrl(String);

impl TryFrom<&str> for EndpointUrl {
    type Error = ConfigError;

    fn try_from(text: &str) -> Result<Self, ConfigError> {
        let rest = text
            .strip_prefix("http://")
            .or_else(|| text.strip_prefix("https://"));
        let fits = rest.is_some_and(|rest| {
            !rest.is_empty() && rest.bytes().all(|byte| byte.is_ascii_graphic())
        }) && text.len() <= MAX_ENDPOINT;
        if fits {
            Ok(Self(text.to_owned()))
        } else {
            Err(ConfigError::Malformed {
                key: "endpoint",
                rule: "an http or https URL of visible ASCII characters",
            })
        }
    }
}

/// The endpoints of the DynamoDB stand-in and the S3 stand-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    dynamodb: EndpointUrl,
    s3: EndpointUrl,
}

impl Endpoints {
    /// The two stand-ins' endpoints.
    #[must_use]
    pub fn new(dynamodb: EndpointUrl, s3: EndpointUrl) -> Self {
        Self { dynamodb, s3 }
    }
}

/// A fault that every write of the stores meets, so a test can show what a
/// store does when an answer is lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Fault {
    /// Each write is sent and applied, and then its answer is reported as
    /// lost, as a timeout after the send would.
    LoseEveryAnswer,
    /// No write is sent, and each is reported as lost.
    DropEveryRequest,
}

/// The policy of stores with a fault: a short deadline, so a write whose
/// every answer is lost settles quickly.
const FAULT_POLICY: Policy = Policy {
    deadline: Duration::from_millis(1_500),
    resend_gap: Duration::from_millis(300),
    ..Policy::STORE
};

impl Stores {
    /// The stores for `config` on the stand-ins at `endpoints`.
    #[must_use]
    pub fn connect_at(config: &HostedConfig, endpoints: &Endpoints) -> Self {
        let (dynamodb, s3) = clients(config, endpoints);
        Self::from_clients(config, dynamodb, s3, Policy::STORE, Faults::None)
    }

    /// The stores for `config` on the stand-ins at `endpoints`, with
    /// `fault` on every write and a deadline of 1.5 seconds.
    #[must_use]
    pub fn connect_at_with_fault(
        config: &HostedConfig,
        endpoints: &Endpoints,
        fault: Fault,
    ) -> Self {
        let faults = match fault {
            Fault::LoseEveryAnswer => Faults::LoseAnswers,
            Fault::DropEveryRequest => Faults::DropRequests,
        };
        let (dynamodb, s3) = clients(config, endpoints);
        Self::from_clients(config, dynamodb, s3, FAULT_POLICY, faults)
    }
}

#[expect(
    clippy::disallowed_methods,
    reason = "connect.rs builds the DynamoDB and S3 clients (SVC-12)"
)]
fn clients(config: &HostedConfig, endpoints: &Endpoints) -> (DynamoClient, S3Client) {
    let region = config.region().as_str().to_owned();
    let dynamodb = aws_sdk_dynamodb::Config::builder()
        .behavior_version(aws_sdk_dynamodb::config::BehaviorVersion::latest())
        .region(aws_sdk_dynamodb::config::Region::new(region.clone()))
        .endpoint_url(&endpoints.dynamodb.0)
        .credentials_provider(aws_sdk_dynamodb::config::Credentials::new(
            ACCESS_KEY_ID,
            SECRET_ACCESS_KEY,
            None,
            None,
            "hangar-stand-ins",
        ))
        .build();
    let s3 = aws_sdk_s3::Config::builder()
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new(region))
        .endpoint_url(&endpoints.s3.0)
        .force_path_style(true)
        .credentials_provider(aws_sdk_s3::config::Credentials::new(
            ACCESS_KEY_ID,
            SECRET_ACCESS_KEY,
            None,
            None,
            "hangar-stand-ins",
        ))
        .build();
    (DynamoClient::from_conf(dynamodb), S3Client::from_conf(s3))
}

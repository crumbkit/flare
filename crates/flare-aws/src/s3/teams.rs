//! [`S3TeamsDocument`]: a deployment's teams document, read from an object
//! that its sync job writes to the stores bucket under `teams/`.
//!
//! flare only reads under `teams/`; the template's managed policy lets it
//! get those objects and nothing more there. The read stops one byte past
//! [`MAX_DOCUMENT_BYTES`], which flare-core then refuses as too large.

use crate::dynamodb::error_code;
use crate::s3::Bucket;
use aws_sdk_s3::operation::RequestId;
use aws_sdk_s3::operation::get_object::GetObjectError;
use flare_core::{DocumentUnreadable, MAX_DOCUMENT_BYTES, TeamsDocument};
use std::fmt;

/// The prefix of every teams document's key.
const TEAMS_PREFIX: &str = "teams/";

/// The longest key S3 stores, in bytes.
const MAX_KEY: usize = 1024;

/// The key of a teams document: `teams/` and at least one more byte, at
/// most 1,024 bytes in all, with no control character.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamsObjectKey(String);

/// Why a teams document's key was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "a teams document's key starts with teams/, has at least one more character, holds no control character and is at most 1024 bytes"
)]
pub struct TeamsKeyError;

impl TryFrom<&str> for TeamsObjectKey {
    type Error = TeamsKeyError;

    fn try_from(text: &str) -> Result<Self, TeamsKeyError> {
        let named = text
            .strip_prefix(TEAMS_PREFIX)
            .is_some_and(|rest| !rest.is_empty());
        if !named || text.len() > MAX_KEY || text.chars().any(char::is_control) {
            return Err(TeamsKeyError);
        }
        Ok(Self(text.to_owned()))
    }
}

impl TeamsObjectKey {
    /// The key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TeamsObjectKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The teams document at one key of the stores bucket.
#[derive(Debug, Clone)]
pub struct S3TeamsDocument {
    bucket: Bucket,
    key: TeamsObjectKey,
}

impl S3TeamsDocument {
    pub(crate) fn new(bucket: Bucket, key: TeamsObjectKey) -> Self {
        Self { bucket, key }
    }
}

impl TeamsDocument for S3TeamsDocument {
    async fn read(&self) -> Result<Vec<u8>, DocumentUnreadable> {
        let key = self.key.as_str();
        let read = self
            .bucket
            .client()
            .get_object()
            .bucket(self.bucket.name())
            .key(key)
            .send()
            .await;
        let output = match read {
            Ok(output) => output,
            Err(error) => {
                let missing = error
                    .as_service_error()
                    .is_some_and(GetObjectError::is_no_such_key);
                let code = error_code(&error);
                tracing::warn!(
                    call = "GetObject",
                    item = "teams document",
                    code = code.as_str(),
                    request_id = error.request_id().unwrap_or("none"),
                    "an S3 call failed"
                );
                return Err(DocumentUnreadable::new(if missing {
                    format!("the stores bucket has no object {key}")
                } else {
                    format!(
                        "the stores bucket did not answer a read of {key}; check the credentials, the policy and the network"
                    )
                }));
            }
        };
        let mut body = output.body;
        let mut bytes = Vec::new();
        let most = MAX_DOCUMENT_BYTES.saturating_add(1);
        loop {
            match body.try_next().await {
                Ok(Some(chunk)) => {
                    let room = most.saturating_sub(bytes.len());
                    bytes.extend_from_slice(chunk.get(..room.min(chunk.len())).unwrap_or(&[]));
                    if bytes.len() >= most {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    return Err(DocumentUnreadable::new(format!(
                        "the read of {key} from the stores bucket stopped before its end"
                    )));
                }
            }
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests;

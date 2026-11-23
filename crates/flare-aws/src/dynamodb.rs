//! The DynamoDB share and grant stores, and the one table they share.
//!
//! [`Table`] is the only holder of the DynamoDB client. Every read is a
//! consistent `GetItem` or `Query`, and every write is one
//! `TransactWriteItems` that the retry loop in [`retry`] sends.

mod cancel;
mod grants;
pub(crate) mod items;
mod keys;
mod read;
pub(crate) mod retry;
mod settle;
mod shares;
pub(crate) mod transact;

pub use grants::DynamoGrantStore;
pub use shares::DynamoShareStore;

use crate::dynamodb::cancel::{Classified, Failure, Reason, classify};
use crate::dynamodb::retry::{Policy, Token};
use crate::dynamodb::transact::Transaction;
use aws_sdk_dynamodb::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_dynamodb::operation::RequestId;
use aws_sdk_dynamodb::operation::transact_write_items::TransactWriteItemsError;
use std::fmt;
use std::sync::Arc;

/// A fault that a test injects into every write, so it can show what the
/// store does when an answer is lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Faults {
    /// Every write is sent and answered as usual.
    None,
    /// Every write is sent, and then its answer is reported as lost.
    LoseAnswers,
    /// No write is sent, and each is reported as lost.
    DropRequests,
    /// The first attempt of each file put is sent and applied, and its
    /// answer is replaced with a 500 inside the SDK, so the SDK's own retry
    /// sends it again.
    FailFirstPutAttempts,
}

/// The table, its client, the retry policy and the injected fault.
#[derive(Clone)]
pub(crate) struct Table(Arc<TableParts>);

struct TableParts {
    client: DynamoClient,
    name: String,
    policy: Policy,
    faults: Faults,
}

#[expect(
    clippy::disallowed_types,
    reason = "dynamodb.rs owns the DynamoDB client (SVC-12)"
)]
pub(crate) type DynamoClient = aws_sdk_dynamodb::Client;

impl fmt::Debug for Table {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Table")
            .field("name", &self.0.name)
            .finish_non_exhaustive()
    }
}

impl Table {
    /// The table `name` through `client`.
    pub(crate) fn new(client: DynamoClient, name: &str, policy: Policy, faults: Faults) -> Self {
        Self(Arc::new(TableParts {
            client,
            name: name.to_owned(),
            policy,
            faults,
        }))
    }

    pub(crate) fn client(&self) -> &DynamoClient {
        &self.0.client
    }

    pub(crate) fn name(&self) -> &str {
        &self.0.name
    }

    pub(crate) fn policy(&self) -> &Policy {
        &self.0.policy
    }

    /// Sends `transaction` with `token`, as the store operation
    /// `operation`, and classifies a failure.
    pub(crate) async fn transact(
        &self,
        transaction: &Transaction,
        token: &Token,
        operation: &'static str,
    ) -> Result<(), Classified> {
        if self.0.faults == Faults::DropRequests {
            return Err(Classified::Unknown);
        }
        let sent = self
            .client()
            .transact_write_items()
            .set_transact_items(Some(transaction.items.clone()))
            .client_request_token(token.as_str())
            .send()
            .await;
        match sent {
            Ok(_) if self.0.faults == Faults::LoseAnswers => Err(Classified::Unknown),
            Ok(_) => Ok(()),
            Err(error) => {
                let classified = classify(&transaction.roles, failure(&error));
                if let Classified::Unavailable(code) = &classified {
                    tracing::warn!(
                        call = "TransactWriteItems",
                        operation,
                        code = code.as_str(),
                        request_id = error.request_id().unwrap_or("none"),
                        "a DynamoDB transaction failed"
                    );
                }
                Err(classified)
            }
        }
    }
}

/// How a `TransactWriteItems` call failed.
fn failure<R>(error: &SdkError<TransactWriteItemsError, R>) -> Failure {
    let SdkError::ServiceError(context) = error else {
        return match error {
            SdkError::ConstructionFailure(_) => Failure::Refused("ConstructionFailure".to_owned()),
            _ => Failure::Lost,
        };
    };
    match context.err() {
        TransactWriteItemsError::TransactionCanceledException(cancelled) => Failure::Cancelled(
            cancelled
                .cancellation_reasons()
                .iter()
                .map(|reason| Reason {
                    code: reason.code().map(str::to_owned),
                    old: reason.item().cloned(),
                })
                .collect(),
        ),
        TransactWriteItemsError::TransactionInProgressException(_) => Failure::InProgress,
        TransactWriteItemsError::ProvisionedThroughputExceededException(_)
        | TransactWriteItemsError::ThrottlingException(_)
        | TransactWriteItemsError::RequestLimitExceeded(_) => Failure::Throttled,
        TransactWriteItemsError::InternalServerError(_) => Failure::Lost,
        other => Failure::Refused(other.code().unwrap_or("Unknown").to_owned()),
    }
}

/// The code a failed call names in a log line: the service's error code,
/// or the kind of failure when no answer came.
pub(crate) fn error_code<E: ProvideErrorMetadata, R>(error: &SdkError<E, R>) -> String {
    match error {
        SdkError::ServiceError(context) => context.err().code().unwrap_or("Unknown").to_owned(),
        SdkError::TimeoutError(_) => "TimeoutError".to_owned(),
        SdkError::DispatchFailure(_) => "DispatchFailure".to_owned(),
        SdkError::ResponseError(_) => "ResponseError".to_owned(),
        SdkError::ConstructionFailure(_) => "ConstructionFailure".to_owned(),
        _ => "Unknown".to_owned(),
    }
}

/// Writes bytes as lower-case hexadecimal characters.
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        for nibble in [byte >> 4, byte & 0x0f] {
            let digit = DIGITS.get(usize::from(nibble)).copied().unwrap_or(b'0');
            text.push(char::from(digit));
        }
    }
    text
}

/// 128 random bits as 32 hexadecimal characters, or `None` when the system
/// has no randomness.
pub(crate) fn random_hex() -> Option<String> {
    let mut bytes = [0_u8; 16];
    aws_lc_rs::rand::fill(&mut bytes).ok()?;
    Some(hex(&bytes))
}

#[cfg(test)]
mod testing;

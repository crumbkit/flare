//! Reads a failed transaction: which condition failed, whether a retry may
//! clear it, and whether the write may have been applied.

use crate::dynamodb::items::Item;
use crate::dynamodb::transact::ItemRole;

/// The reason DynamoDB gives for one item of a cancelled transaction, in
/// the order of the transaction's items.
#[derive(Debug, Clone)]
pub(crate) struct Reason {
    /// The reason's code, such as `ConditionalCheckFailed`, or `None` when
    /// the item did not cause the cancellation.
    pub(crate) code: Option<String>,
    /// The item as it was, when the transaction asked for it.
    pub(crate) old: Option<Item>,
}

/// How a call to DynamoDB failed, before the store decides what it means.
#[derive(Debug, Clone)]
pub(crate) enum Failure {
    /// The transaction was cancelled, with one reason per item.
    Cancelled(Vec<Reason>),
    /// Another call with the same token is still in progress.
    InProgress,
    /// DynamoDB refused the call for capacity, so nothing was written.
    Throttled,
    /// The answer was lost after the request may have been sent: a timeout,
    /// a connection error or a 5xx answer.
    Lost,
    /// DynamoDB refused the call for another reason, named by its code.
    Refused(String),
}

/// What a failure means to the store.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Classified {
    /// A condition failed on the item with this role. The store reads it
    /// as a refusal, or as a reason to read again.
    Conflict {
        /// The role of the item whose condition failed.
        role: ItemRole,
        /// The item as it was, when the transaction asked for it.
        old: Option<Item>,
    },
    /// Nothing was written and a fresh attempt may succeed.
    Retry,
    /// The write may have been applied.
    Unknown,
    /// Nothing was written and a retry would fail the same way. The code
    /// names the reason for the log.
    Unavailable(String),
}

const CONDITION_FAILED: &str = "ConditionalCheckFailed";

/// The reason codes that a fresh attempt may clear.
const RETRIED: [&str; 3] = [
    "TransactionConflict",
    "ThrottlingError",
    "ProvisionedThroughputExceeded",
];

/// The rank of a failed condition: a lower rank is reported first. A failed
/// admin check is stale authority, which comes before a changed grant, and
/// a changed grant or a taken share id comes before a limit.
fn rank(role: ItemRole) -> u8 {
    match role {
        ItemRole::AdminCheck => 0,
        ItemRole::GrantItem
        | ItemRole::ShareItem
        | ItemRole::CodeItem
        | ItemRole::TokenItem
        | ItemRole::PairItem
        | ItemRole::SessionGuard
        | ItemRole::SealingKeyItem
        | ItemRole::CommentItem
        | ItemRole::CommentPointer
        | ItemRole::CommentKeySlot
        | ItemRole::CursorItem => 1,
        ItemRole::GrantCount
        | ItemRole::ShareCount
        | ItemRole::LabelSlot
        | ItemRole::OldToken
        | ItemRole::Sequence
        | ItemRole::OldSealingKey
        | ItemRole::OldSession
        | ItemRole::CommentCount
        | ItemRole::CursorCount => 2,
        ItemRole::Version
        | ItemRole::OwnerEntry
        | ItemRole::GranteeEntry
        | ItemRole::LinkItem
        | ItemRole::SessionItem
        | ItemRole::SessionLink
        | ItemRole::CommentEvent => 3,
    }
}

/// Classifies a failure of a transaction whose items have `roles`.
///
/// A failed condition is a conflict on the item of the lowest rank. A
/// transaction conflict or a throttle, with no failed condition, is
/// retried. Any other reason, and a list of reasons that does not match
/// the items, is unavailable.
pub(crate) fn classify(roles: &[ItemRole], failure: Failure) -> Classified {
    let reasons = match failure {
        Failure::Cancelled(reasons) => reasons,
        Failure::InProgress | Failure::Lost => return Classified::Unknown,
        Failure::Throttled => return Classified::Retry,
        Failure::Refused(code) => return Classified::Unavailable(code),
    };
    if reasons.len() != roles.len() {
        return Classified::Unavailable("CancellationReasonsDoNotMatchItems".to_owned());
    }
    let mut conflict: Option<(ItemRole, Option<Item>)> = None;
    let mut retry = false;
    let mut other: Option<String> = None;
    for (role, reason) in roles.iter().copied().zip(reasons) {
        match reason.code.as_deref() {
            None | Some("None") => {}
            Some(CONDITION_FAILED) => {
                if conflict
                    .as_ref()
                    .is_none_or(|(held, _)| rank(role) < rank(*held))
                {
                    conflict = Some((role, reason.old));
                }
            }
            Some(code) if RETRIED.contains(&code) => retry = true,
            Some(code) => other = Some(code.to_owned()),
        }
    }
    match (conflict, other, retry) {
        (Some((role, old)), _, _) => Classified::Conflict { role, old },
        (None, Some(code), _) => Classified::Unavailable(code),
        (None, None, true) => Classified::Retry,
        (None, None, false) => Classified::Unavailable("CancelledWithoutReason".to_owned()),
    }
}

#[cfg(test)]
mod tests;

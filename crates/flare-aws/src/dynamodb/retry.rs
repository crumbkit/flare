//! The retry loop that every write runs, generic over the attempt it
//! retries.
//!
//! An attempt is built from a fresh read, then sent with a token. A send
//! whose outcome is unknown is sent again with the same token, so a request
//! that DynamoDB already applied is answered as a replay and not applied
//! twice. The first resend waits [`Policy::resend_gap`], and each later one
//! waits twice as long as the one before, up to [`MAX_RESEND_WAIT`] times
//! the gap. A refusal of a resend is definite, because a token whose
//! request was applied is answered as a replay of that success, so the loop
//! returns it. A definite cancellation that a fresh read may clear rebuilds
//! the attempt with a new token, at most [`Policy::cancellations`] times.
//! One deadline bounds the whole call. When it ends with the outcome
//! unknown, or a resend meets a cancellation or an unavailable store, the
//! loop settles the outcome with a consistent read: a match is success, and
//! anything else is unavailable, which for a write means the outcome is
//! unknown.
//!
//! This module owns flare-aws's clock: it is the only place that sleeps.

use std::future::Future;
use std::time::Duration;
use tokio::time::{Instant, timeout};

/// The durations and counts of the loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Policy {
    /// How long one store call may retry, from its start.
    pub(crate) deadline: Duration,
    /// How long the first resend of a token waits after its unknown answer.
    pub(crate) resend_gap: Duration,
    /// How many attempts a call builds before it gives up on cancellations.
    pub(crate) cancellations: u32,
    /// The first wait between two attempts. Each later wait doubles it.
    pub(crate) backoff: Duration,
}

impl Policy {
    /// The policy of the stores: a deadline of 20 seconds, a first resend 6
    /// seconds after an unknown answer, and at most 5 attempts.
    pub(crate) const STORE: Self = Self {
        deadline: Duration::from_secs(20),
        resend_gap: Duration::from_secs(6),
        cancellations: 5,
        backoff: Duration::from_millis(25),
    };
}

/// The longest wait before a resend, as a multiple of
/// [`Policy::resend_gap`].
pub(crate) const MAX_RESEND_WAIT: u32 = 4;

/// A `ClientRequestToken`: 128 random bits as 32 hexadecimal characters.
/// One token belongs to one built attempt and every resend of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token(String);

impl Token {
    /// A new random token, or `None` when the system has no randomness.
    pub(crate) fn fresh() -> Option<Self> {
        crate::dynamodb::random_hex().map(Self)
    }

    /// The token as text.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// The answer to one build or one send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Sent<T, R> {
    /// The write was applied, or the read answered the call.
    Done(T),
    /// The store refused the call, and nothing was written.
    Refused(R),
    /// Nothing was written and a fresh attempt may succeed.
    Again,
    /// The write may have been applied.
    Unknown,
    /// Nothing was written and the store cannot answer.
    Unavailable,
}

/// What a consistent read says about an attempt whose outcome is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Settled<T> {
    /// The store holds what the attempt wrote.
    Matches(T),
    /// The store holds something else.
    Differs,
    /// The read failed.
    Unavailable,
}

/// The answer of a whole call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Final<T, R> {
    /// The write was applied.
    Done(T),
    /// The store refused the call, and nothing was written.
    Refused(R),
    /// The store cannot answer. After a write the outcome is unknown.
    Unavailable,
}

/// One kind of write that the loop retries.
pub(crate) trait Attempt: Sync {
    /// What a send needs: the request and what the settling read compares.
    type Built: Send + Sync;
    /// The answer of an applied write.
    type Done: Send;
    /// The store's refusal.
    type Refused: Send;

    /// Reads what the write depends on and builds the request. A read can
    /// also answer the call, such as a share that does not exist.
    fn build(
        &self,
    ) -> impl Future<Output = Result<Self::Built, Sent<Self::Done, Self::Refused>>> + Send;

    /// Sends the built request with `token`.
    fn send(
        &self,
        built: &Self::Built,
        token: &Token,
    ) -> impl Future<Output = Sent<Self::Done, Self::Refused>> + Send;

    /// Reads, with a consistent read, whether the store holds what `built`
    /// writes.
    fn settle(&self, built: &Self::Built) -> impl Future<Output = Settled<Self::Done>> + Send;
}

/// Waits until `until`.
async fn pause(until: Instant) {
    let wait = until.saturating_duration_since(Instant::now());
    #[expect(
        clippy::disallowed_methods,
        reason = "retry.rs is flare-aws's clock owner: it waits between attempts and before a resend"
    )]
    tokio::time::sleep(wait).await;
}

/// The wait before attempt `number` after a cancellation: the policy's
/// backoff doubled for each earlier cancellation, with random jitter that
/// keeps from half to all of it.
fn backoff(policy: &Policy, number: u32) -> Duration {
    let full = policy
        .backoff
        .saturating_mul(1_u32 << number.saturating_sub(1).min(10));
    let mut byte = [0_u8; 1];
    let jitter = match aws_lc_rs::rand::fill(&mut byte) {
        Ok(()) => {
            let [value] = byte;
            value
        }
        Err(_) => u8::MAX,
    };
    full / 2 + (full / 2) * u32::from(jitter) / 255
}

/// Waits the backoff before attempt `number` of a read that asks again,
/// such as a batch read whose keys DynamoDB left unprocessed.
pub(crate) async fn back_off(policy: &Policy, number: u32) {
    pause(Instant::now() + backoff(policy, number)).await;
}

/// What a series of sends with one token ended in.
enum Answer<T, R> {
    /// The call is answered.
    Final(Final<T, R>),
    /// The attempt was cancelled and nothing of it was written, so the loop
    /// builds a fresh one.
    Again,
}

/// Settles an unknown outcome, and answers `otherwise` when the store holds
/// something else.
async fn settle_or<A: Attempt>(
    policy: &Policy,
    attempt: &A,
    built: &A::Built,
    otherwise: Answer<A::Done, A::Refused>,
) -> Answer<A::Done, A::Refused> {
    match timeout(policy.resend_gap, attempt.settle(built)).await {
        Ok(Settled::Matches(done)) => Answer::Final(Final::Done(done)),
        Ok(Settled::Differs) => otherwise,
        Ok(Settled::Unavailable) | Err(_) => Answer::Final(Final::Unavailable),
    }
}

/// Sends `built` with one token until it is answered, resending after an
/// unknown outcome, until the deadline.
async fn send_until_answered<A: Attempt>(
    policy: &Policy,
    attempt: &A,
    built: &A::Built,
    deadline: Instant,
) -> Answer<A::Done, A::Refused> {
    let Some(token) = Token::fresh() else {
        return Answer::Final(Final::Unavailable);
    };
    let mut wait = policy.resend_gap;
    let longest = policy.resend_gap.saturating_mul(MAX_RESEND_WAIT);
    let mut unknown = false;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let Ok(sent) = timeout(remaining, attempt.send(built, &token)).await else {
            unknown = true;
            break;
        };
        match sent {
            Sent::Done(done) => return Answer::Final(Final::Done(done)),
            Sent::Unknown => {
                unknown = true;
                pause((Instant::now() + wait).min(deadline)).await;
                wait = wait.saturating_mul(2).min(longest);
            }
            Sent::Refused(refused) => return Answer::Final(Final::Refused(refused)),
            Sent::Again if unknown => {
                return settle_or(policy, attempt, built, Answer::Again).await;
            }
            Sent::Again => return Answer::Again,
            Sent::Unavailable if unknown => {
                let unavailable = Answer::Final(Final::Unavailable);
                return settle_or(policy, attempt, built, unavailable).await;
            }
            Sent::Unavailable => return Answer::Final(Final::Unavailable),
        }
    }
    if unknown {
        settle_or(policy, attempt, built, Answer::Final(Final::Unavailable)).await
    } else {
        Answer::Final(Final::Unavailable)
    }
}

/// Runs `attempt` under `policy` and answers the call.
pub(crate) async fn run<A: Attempt>(policy: &Policy, attempt: &A) -> Final<A::Done, A::Refused> {
    let deadline = Instant::now() + policy.deadline;
    let mut cancellations = 0_u32;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let answer = match timeout(remaining, attempt.build()).await {
            Err(_) => return Final::Unavailable,
            Ok(Ok(built)) => send_until_answered(policy, attempt, &built, deadline).await,
            Ok(Err(Sent::Done(done))) => return Final::Done(done),
            Ok(Err(Sent::Refused(refused))) => return Final::Refused(refused),
            Ok(Err(Sent::Again)) => Answer::Again,
            Ok(Err(Sent::Unknown | Sent::Unavailable)) => return Final::Unavailable,
        };
        match answer {
            Answer::Final(answer) => return answer,
            Answer::Again => {
                cancellations = cancellations.saturating_add(1);
                if cancellations >= policy.cancellations {
                    return Final::Unavailable;
                }
                let next = Instant::now() + backoff(policy, cancellations);
                pause(next.min(deadline)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests;

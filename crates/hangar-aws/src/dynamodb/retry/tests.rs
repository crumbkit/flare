use super::{Attempt, Final, Policy, Sent, Settled, Token, run};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A policy with short durations, so a test that runs to the deadline ends
/// quickly.
fn quick(deadline_ms: u64) -> Policy {
    Policy {
        deadline: Duration::from_millis(deadline_ms),
        resend_gap: Duration::from_millis(40),
        cancellations: 5,
        backoff: Duration::from_millis(1),
    }
}

/// An attempt that answers each send from a script, and records each build,
/// each token and each moment it was sent.
#[derive(Default)]
struct Scripted {
    sends: Mutex<VecDeque<Sent<u32, &'static str>>>,
    builds: Mutex<u32>,
    tokens: Mutex<Vec<(u32, String, Instant)>>,
    settle: Option<Settled<u32>>,
    settled: Mutex<u32>,
    /// Whether a send past the script commits and loses its answer.
    then: Option<Sent<u32, &'static str>>,
}

impl Scripted {
    fn new(sends: Vec<Sent<u32, &'static str>>) -> Self {
        Self {
            sends: Mutex::new(sends.into()),
            ..Self::default()
        }
    }

    fn tokens(&self) -> Vec<(u32, String, Instant)> {
        self.tokens.lock().unwrap().clone()
    }
}

impl Attempt for Scripted {
    type Built = u32;
    type Done = u32;
    type Refused = &'static str;

    async fn build(&self) -> Result<u32, Sent<u32, &'static str>> {
        let mut builds = self.builds.lock().unwrap();
        *builds += 1;
        Ok(*builds)
    }

    async fn send(&self, built: &u32, token: &Token) -> Sent<u32, &'static str> {
        self.tokens
            .lock()
            .unwrap()
            .push((*built, token.as_str().to_owned(), Instant::now()));
        let next = self.sends.lock().unwrap().pop_front();
        next.or_else(|| self.then.clone())
            .unwrap_or(Sent::Unavailable)
    }

    async fn settle(&self, _built: &u32) -> Settled<u32> {
        *self.settled.lock().unwrap() += 1;
        self.settle.clone().unwrap_or(Settled::Unavailable)
    }
}

#[tokio::test]
async fn an_answered_send_is_returned_without_a_resend() {
    let attempt = Scripted::new(vec![Sent::Done(7)]);
    assert_eq!(run(&quick(500), &attempt).await, Final::Done(7));
    assert_eq!(attempt.tokens().len(), 1);
}

#[tokio::test]
async fn a_resend_after_an_unknown_outcome_reuses_its_token() {
    let attempt = Scripted::new(vec![Sent::Unknown, Sent::Unknown, Sent::Done(3)]);
    assert_eq!(run(&quick(2_000), &attempt).await, Final::Done(3));
    let tokens = attempt.tokens();
    assert_eq!(tokens.len(), 3);
    assert!(
        tokens
            .iter()
            .all(|(built, token, _)| *built == 1 && *token == tokens[0].1)
    );
}

#[tokio::test]
async fn a_resend_waits_the_gap_after_the_first_send_of_its_token() {
    let attempt = Scripted::new(vec![Sent::Unknown, Sent::Done(3)]);
    let policy = quick(2_000);
    run(&policy, &attempt).await;
    let tokens = attempt.tokens();
    assert!(tokens[1].2.duration_since(tokens[0].2) >= policy.resend_gap);
}

#[tokio::test]
async fn a_fresh_attempt_after_a_cancellation_rebuilds_and_takes_a_new_token() {
    let attempt = Scripted::new(vec![Sent::Again, Sent::Done(4)]);
    assert_eq!(run(&quick(2_000), &attempt).await, Final::Done(4));
    let tokens = attempt.tokens();
    assert_eq!(*attempt.builds.lock().unwrap(), 2);
    assert_eq!((tokens[0].0, tokens[1].0), (1, 2));
    assert_ne!(tokens[0].1, tokens[1].1);
}

#[tokio::test]
async fn cancellations_stop_after_five_attempts_with_unavailable() {
    let mut attempt = Scripted::new(Vec::new());
    attempt.then = Some(Sent::Again);
    assert_eq!(run(&quick(5_000), &attempt).await, Final::Unavailable);
    assert_eq!(*attempt.builds.lock().unwrap(), 5);
}

#[tokio::test]
async fn a_refusal_is_returned_and_nothing_is_settled() {
    let attempt = Scripted::new(vec![Sent::Refused("taken")]);
    assert_eq!(run(&quick(500), &attempt).await, Final::Refused("taken"));
    assert_eq!(*attempt.settled.lock().unwrap(), 0);
}

#[tokio::test]
async fn a_send_that_commits_and_loses_every_answer_is_settled_as_done_at_the_deadline() {
    let mut attempt = Scripted::new(Vec::new());
    attempt.then = Some(Sent::Unknown);
    attempt.settle = Some(Settled::Matches(9));
    let started = Instant::now();
    assert_eq!(run(&quick(300), &attempt).await, Final::Done(9));
    assert!(started.elapsed() < Duration::from_millis(1_500));
    assert_eq!(*attempt.settled.lock().unwrap(), 1);
    let tokens = attempt.tokens();
    assert!(tokens.len() >= 2);
    assert!(tokens.iter().all(|(_, token, _)| *token == tokens[0].1));
}

#[tokio::test]
async fn an_unknown_outcome_whose_read_finds_no_match_is_unavailable() {
    let mut attempt = Scripted::new(Vec::new());
    attempt.then = Some(Sent::Unknown);
    attempt.settle = Some(Settled::Differs);
    assert_eq!(run(&quick(300), &attempt).await, Final::Unavailable);
}

#[tokio::test]
async fn a_refusal_after_an_unknown_outcome_is_settled_first() {
    let mut attempt = Scripted::new(vec![Sent::Unknown, Sent::Refused("taken")]);
    attempt.settle = Some(Settled::Matches(5));
    assert_eq!(run(&quick(2_000), &attempt).await, Final::Done(5));
    let mut attempt = Scripted::new(vec![Sent::Unknown, Sent::Refused("taken")]);
    attempt.settle = Some(Settled::Differs);
    assert_eq!(run(&quick(2_000), &attempt).await, Final::Refused("taken"));
}

#[tokio::test]
async fn the_deadline_bounds_a_send_that_never_answers() {
    struct Hangs;
    impl Attempt for Hangs {
        type Built = ();
        type Done = ();
        type Refused = ();
        async fn build(&self) -> Result<(), Sent<(), ()>> {
            Ok(())
        }
        async fn send(&self, (): &(), _token: &Token) -> Sent<(), ()> {
            std::future::pending().await
        }
        async fn settle(&self, (): &()) -> Settled<()> {
            Settled::Differs
        }
    }
    let started = Instant::now();
    assert_eq!(run(&quick(200), &Hangs).await, Final::Unavailable);
    assert!(started.elapsed() < Duration::from_millis(1_500));
}

#[test]
fn a_token_is_32_hexadecimal_characters_and_never_repeats() {
    let first = Token::fresh().unwrap();
    let second = Token::fresh().unwrap();
    assert_eq!(first.as_str().len(), 32);
    assert!(first.as_str().bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_ne!(first.as_str(), second.as_str());
}

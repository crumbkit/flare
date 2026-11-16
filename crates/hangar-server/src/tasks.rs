//! The one module that spawns tasks and waits on timers (SVC-1, BASE-11).
//!
//! [`Tasks`] owns every task the server starts: one per connection and the
//! sweeper. [`Listeners::serve`](crate::Listeners::serve) drains them, then
//! aborts what remains and waits for each to end before it returns, so no
//! task outlives `serve`. A request runs inside its connection's task;
//! [`caught`] turns a panic in it into an error that the caller answers
//! with 500, so the connection and the server keep serving.

use std::any::Any;
use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::Instant as TokioInstant;

/// The server's tasks.
#[derive(Debug, Default)]
pub(crate) struct Tasks {
    set: JoinSet<()>,
}

impl Tasks {
    /// Starts `task` and keeps it until it ends or is aborted.
    pub(crate) fn spawn(&mut self, task: impl Future<Output = ()> + Send + 'static) {
        self.set.spawn(task);
    }

    /// Waits for one task to end. Never resolves while there is none, so a
    /// `select!` can poll it safely.
    pub(crate) async fn reap(&mut self) {
        if self.set.join_next().await.is_none() {
            std::future::pending::<()>().await;
        }
    }

    /// Waits for every task to end, for at most `limit`, then aborts the
    /// rest and waits for them to end.
    pub(crate) async fn finish(mut self, limit: Duration) {
        let all = async { while self.set.join_next().await.is_some() {} };
        if within(limit, all).await.is_none() {
            self.set.abort_all();
            while self.set.join_next().await.is_some() {}
        }
    }

    /// Aborts every task and waits for each to end.
    pub(crate) async fn abort(mut self) {
        self.set.abort_all();
        while self.set.join_next().await.is_some() {}
    }
}

/// Resolves once `every` has passed, again and again: the sweeper's beat.
pub(crate) async fn tick(every: Duration) {
    tokio::time::sleep_until(TokioInstant::now() + every).await;
}

/// Resolves when `deadline` passes.
pub(crate) async fn until(deadline: std::time::Instant) {
    tokio::time::sleep_until(TokioInstant::from_std(deadline)).await;
}

/// Runs `future` for at most `limit`; `None` when the limit passed first.
pub(crate) async fn within<F: Future>(limit: Duration, future: F) -> Option<F::Output> {
    tokio::time::timeout(limit, future).await.ok()
}

/// A panic caught while a request ran.
#[derive(Debug)]
pub(crate) struct Panicked;

/// Runs `future` and turns a panic in it into [`Panicked`]. The request's
/// tables are behind mutexes whose poisoning each table answers as
/// unavailable, so state after a caught panic is never read as good.
pub(crate) fn caught<F: Future>(future: F) -> impl Future<Output = Result<F::Output, Panicked>> {
    Caught {
        inner: Box::pin(future),
    }
}

struct Caught<F> {
    inner: Pin<Box<F>>,
}

impl<F: Future> Future for Caught<F> {
    type Output = Result<F::Output, Panicked>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = &mut self.inner;
        match catch_unwind(AssertUnwindSafe(|| inner.as_mut().poll(cx))) {
            Ok(Poll::Ready(value)) => Poll::Ready(Ok(value)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => {
                drop::<Box<dyn Any + Send>>(payload);
                Poll::Ready(Err(Panicked))
            }
        }
    }
}

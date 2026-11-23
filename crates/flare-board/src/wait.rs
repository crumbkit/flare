//! A one-shot future that a thread completes, and the gate that bounds how
//! many board children run at once.
//!
//! Shortcut: flare-core has no runtime, so each blocking child runs on a
//! new thread of its own, and the thread completes a hand-written one-shot
//! future and wakes its waker. This handles any executor, including
//! flare-core's `block_on`, and it costs one thread per child. It misses a
//! runtime's pool of blocking threads; `spawn_blocking` replaces it when
//! flare-server brings a runtime.
//!
//! Dropping the future before it resolves cancels its [`Cancel`], which
//! stops the child's process group, and then blocks until the thread has
//! ended. So a caller that drops a call, as a request deadline or a
//! server's drain does, knows that no child of that call still runs and
//! that a write it started has either committed or never will. The block
//! lasts as long as the stop: the child's grace after SIGTERM and its reap.

use crumbkit_process::Cancel;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

/// How often a thread that waits for a place checks whether its call was
/// dropped. Shortcut: the gate's condition variable is not woken by a
/// cancel, so the wait looks at the cancel this often instead.
const CANCEL_CHECK: Duration = Duration::from_millis(20);

/// The state a thread and the future it completes share.
struct Slot<T> {
    value: Option<T>,
    waker: Option<Waker>,
    /// The thread ended without a value, because the work panicked.
    gone: bool,
    /// The thread has ended, with or without a value.
    done: bool,
}

/// The slot and the condition variable that a dropped future waits on.
struct Shared<T> {
    slot: Mutex<Slot<T>>,
    ended: Condvar,
}

/// A future that resolves to the value its thread produced, or to `None`
/// when the thread panicked before it produced one.
pub(crate) struct OnThread<T> {
    shared: Arc<Shared<T>>,
    stop: Cancel,
}

/// Completes the slot when it drops: with the value when one was given,
/// and as gone otherwise, so a panic in the work still wakes the future.
struct Completer<T> {
    shared: Arc<Shared<T>>,
    value: Option<T>,
}

impl<T> Drop for Completer<T> {
    fn drop(&mut self) {
        let mut slot = self
            .shared
            .slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        slot.value = self.value.take();
        slot.gone = slot.value.is_none();
        slot.done = true;
        if let Some(waker) = slot.waker.take() {
            waker.wake();
        }
        self.shared.ended.notify_all();
    }
}

/// Runs `work` on a new thread and returns the future of its value. The
/// work watches `stop`, which the future cancels when it is dropped before
/// it resolves. A thread that cannot be started resolves the future to
/// `None` at once.
pub(crate) fn on_thread<T: Send + 'static>(
    stop: Cancel,
    work: impl FnOnce() -> T + Send + 'static,
) -> OnThread<T> {
    let shared = Arc::new(Shared {
        slot: Mutex::new(Slot {
            value: None,
            waker: None,
            gone: false,
            done: false,
        }),
        ended: Condvar::new(),
    });
    let completer = Completer {
        shared: Arc::clone(&shared),
        value: None,
    };
    let started = std::thread::Builder::new()
        .name("flare-board".to_owned())
        .spawn(move || {
            let mut completer = completer;
            completer.value = Some(work());
        });
    if started.is_err() {
        let mut slot = shared.slot.lock().unwrap_or_else(PoisonError::into_inner);
        slot.gone = true;
        slot.done = true;
    }
    OnThread { shared, stop }
}

impl<T> Future for OnThread<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<T>> {
        let mut slot = self
            .shared
            .slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(value) = slot.value.take() {
            return Poll::Ready(Some(value));
        }
        if slot.gone {
            return Poll::Ready(None);
        }
        slot.waker = Some(context.waker().clone());
        Poll::Pending
    }
}

impl<T> Drop for OnThread<T> {
    /// Stops the work when it has not ended, and waits until its thread
    /// has.
    fn drop(&mut self) {
        let mut slot = self
            .shared
            .slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if slot.done {
            return;
        }
        self.stop.cancel();
        while !slot.done {
            slot = self
                .shared
                .ended
                .wait(slot)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// A counting gate: at most `places` holders at once. A thread that wants a
/// place waits on a condition variable until one is free, its deadline
/// passes or its call is dropped.
#[derive(Debug)]
pub(crate) struct Gate {
    places: usize,
    held: Mutex<usize>,
    freed: Condvar,
}

/// One place in a [`Gate`], given back when it drops, so a thread that
/// panics while it holds a place still frees it.
pub(crate) struct Place<'a> {
    gate: &'a Gate,
}

impl Gate {
    pub(crate) fn new(places: usize) -> Self {
        Self {
            places,
            held: Mutex::new(0),
            freed: Condvar::new(),
        }
    }

    /// Waits until `deadline` for a free place and takes it, or answers
    /// `None` when no place freed in time or `stop` was cancelled.
    pub(crate) fn enter_within(&self, deadline: Instant, stop: &Cancel) -> Option<Place<'_>> {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        while *held >= self.places {
            if stop.is_cancelled() {
                return None;
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            if left.is_zero() {
                return None;
            }
            held = self
                .freed
                .wait_timeout(held, left.min(CANCEL_CHECK))
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        if stop.is_cancelled() {
            return None;
        }
        *held = held.saturating_add(1);
        Some(Place { gate: self })
    }
}

impl Drop for Place<'_> {
    fn drop(&mut self) {
        let mut held = self
            .gate
            .held
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *held = held.saturating_sub(1);
        self.gate.freed.notify_one();
    }
}

#[cfg(test)]
mod tests;

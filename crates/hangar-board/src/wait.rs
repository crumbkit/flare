//! A one-shot future that a thread completes, and the gate that bounds how
//! many board children run at once.
//!
//! Shortcut: hangar-core has no runtime, so each blocking child runs on a
//! new thread of its own, and the thread completes a hand-written one-shot
//! future and wakes its waker. This handles any executor, including
//! hangar-core's `block_on`, and it costs one thread per child. It misses a
//! runtime's pool of blocking threads; `spawn_blocking` replaces it when
//! hangar-server brings a runtime.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::task::{Context, Poll, Waker};

/// The state a thread and the future it completes share.
struct Slot<T> {
    value: Option<T>,
    waker: Option<Waker>,
    /// The thread ended without a value, because the work panicked.
    gone: bool,
}

/// A future that resolves to the value its thread produced, or to `None`
/// when the thread panicked before it produced one.
pub(crate) struct OnThread<T> {
    shared: Arc<Mutex<Slot<T>>>,
}

/// Completes the slot when it drops: with the value when one was given,
/// and as gone otherwise, so a panic in the work still wakes the future.
struct Completer<T> {
    shared: Arc<Mutex<Slot<T>>>,
    value: Option<T>,
}

impl<T> Drop for Completer<T> {
    fn drop(&mut self) {
        let mut slot = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        slot.value = self.value.take();
        slot.gone = slot.value.is_none();
        if let Some(waker) = slot.waker.take() {
            waker.wake();
        }
    }
}

/// Runs `work` on a new thread and returns the future of its value. A
/// thread that cannot be started resolves the future to `None` at once.
pub(crate) fn on_thread<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> OnThread<T> {
    let shared = Arc::new(Mutex::new(Slot {
        value: None,
        waker: None,
        gone: false,
    }));
    let completer = Completer {
        shared: Arc::clone(&shared),
        value: None,
    };
    let started = std::thread::Builder::new()
        .name("hangar-board".to_owned())
        .spawn(move || {
            let mut completer = completer;
            completer.value = Some(work());
        });
    if started.is_err() {
        let mut slot = shared.lock().unwrap_or_else(PoisonError::into_inner);
        slot.gone = true;
    }
    OnThread { shared }
}

impl<T> Future for OnThread<T> {
    type Output = Option<T>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<T>> {
        let mut slot = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
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

/// A counting gate: at most `places` holders at once. A thread that wants a
/// place waits on a condition variable until one is free.
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

    /// Waits for a free place and takes it.
    pub(crate) fn enter(&self) -> Place<'_> {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        while *held >= self.places {
            held = self
                .freed
                .wait(held)
                .unwrap_or_else(PoisonError::into_inner);
        }
        *held = held.saturating_add(1);
        Place { gate: self }
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

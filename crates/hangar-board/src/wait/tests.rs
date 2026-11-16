use super::{Gate, on_thread};
use crumbkit_process::Cancel;
use hangar_core::contract::block_on;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Waker};
use std::time::{Duration, Instant};

/// A deadline that no test reaches.
fn far() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

/// A cancel that nothing cancels.
fn go() -> Cancel {
    Cancel::new()
}

/// Lets another thread run for `millis`.
fn pause(millis: u64) {
    #[expect(
        clippy::disallowed_methods,
        reason = "the test gives another thread time to reach its wait, which shows no signal"
    )]
    std::thread::sleep(Duration::from_millis(millis));
}

#[test]
fn a_future_on_a_thread_resolves_to_the_threads_value() {
    assert_eq!(block_on(on_thread(go(), || 7)), Some(7));
}

#[test]
fn a_future_whose_thread_panics_resolves_to_none() {
    let answer: Option<u8> = block_on(on_thread(go(), || panic!("the work failed")));
    assert_eq!(answer, None);
}

#[test]
fn dropping_an_unfinished_future_cancels_its_work_and_waits_for_the_thread() {
    let stop = Cancel::new();
    let watched = stop.clone();
    let ended = Arc::new(AtomicBool::new(false));
    let marked = Arc::clone(&ended);
    let mut future = Box::pin(on_thread(stop, move || {
        while !watched.is_cancelled() {
            pause(5);
        }
        pause(50);
        marked.store(true, Ordering::SeqCst);
    }));
    let mut context = Context::from_waker(Waker::noop());
    assert!(Pin::as_mut(&mut future).poll(&mut context).is_pending());
    drop(future);
    assert!(ended.load(Ordering::SeqCst));
}

#[test]
fn a_gate_holds_no_more_than_its_places_at_once() {
    let gate = Arc::new(Gate::new(2));
    let inside = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|scope| {
        for _ in 0..6 {
            scope.spawn(|| {
                let _place = gate.enter_within(far(), &go()).unwrap();
                let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                most.fetch_max(now, Ordering::SeqCst);
                std::thread::yield_now();
                inside.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    assert!(most.load(Ordering::SeqCst) <= 2);
}

#[test]
fn a_place_is_freed_when_its_holder_panics() {
    let gate = Arc::new(Gate::new(1));
    let held = Arc::clone(&gate);
    #[expect(
        clippy::disallowed_methods,
        reason = "the test joins the thread that holds the place"
    )]
    let _ = std::thread::spawn(move || {
        let _place = held.enter_within(far(), &go()).unwrap();
        panic!("the holder failed");
    })
    .join();
    let _place = gate.enter_within(far(), &go()).unwrap();
}

#[test]
fn a_gate_with_no_free_place_answers_none_at_the_deadline() {
    let gate = Gate::new(1);
    let _held = gate.enter_within(far(), &go()).unwrap();
    let started = Instant::now();
    let answer = gate.enter_within(Instant::now() + Duration::from_millis(100), &go());
    assert!(answer.is_none());
    assert!(started.elapsed() >= Duration::from_millis(100));
}

#[test]
fn a_waiting_caller_whose_stop_is_cancelled_gets_no_place() {
    let gate = Gate::new(1);
    let held = gate.enter_within(far(), &go()).unwrap();
    let stop = Cancel::new();
    std::thread::scope(|scope| {
        let waiting = scope.spawn(|| gate.enter_within(far(), &stop).is_some());
        pause(50);
        stop.cancel();
        assert!(!waiting.join().unwrap());
    });
    drop(held);
}

#[test]
fn a_place_freed_before_the_deadline_lets_a_waiting_caller_in() {
    let gate = Gate::new(1);
    let held = gate.enter_within(far(), &go()).unwrap();
    std::thread::scope(|scope| {
        let waiting = scope.spawn(|| gate.enter_within(far(), &go()).is_some());
        drop(held);
        assert!(waiting.join().unwrap());
    });
}

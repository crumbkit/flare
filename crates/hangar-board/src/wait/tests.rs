use super::{Gate, on_thread};
use hangar_core::contract::block_on;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// A deadline that no test reaches.
fn far() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

#[test]
fn a_future_on_a_thread_resolves_to_the_threads_value() {
    assert_eq!(block_on(on_thread(|| 7)), Some(7));
}

#[test]
fn a_future_whose_thread_panics_resolves_to_none() {
    let answer: Option<u8> = block_on(on_thread(|| panic!("the work failed")));
    assert_eq!(answer, None);
}

#[test]
fn a_gate_holds_no_more_than_its_places_at_once() {
    let gate = Arc::new(Gate::new(2));
    let inside = Arc::new(AtomicUsize::new(0));
    let most = Arc::new(AtomicUsize::new(0));
    std::thread::scope(|scope| {
        for _ in 0..6 {
            scope.spawn(|| {
                let _place = gate.enter_within(far()).unwrap();
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
    let _ = std::thread::spawn(move || {
        let _place = held.enter_within(far()).unwrap();
        panic!("the holder failed");
    })
    .join();
    let _place = gate.enter_within(far()).unwrap();
}

#[test]
fn a_gate_with_no_free_place_answers_none_at_the_deadline() {
    let gate = Gate::new(1);
    let _held = gate.enter_within(far()).unwrap();
    let started = Instant::now();
    let answer = gate.enter_within(Instant::now() + Duration::from_millis(100));
    assert!(answer.is_none());
    assert!(started.elapsed() >= Duration::from_millis(100));
}

#[test]
fn a_place_freed_before_the_deadline_lets_a_waiting_caller_in() {
    let gate = Gate::new(1);
    let held = gate.enter_within(far()).unwrap();
    std::thread::scope(|scope| {
        let waiting = scope.spawn(|| gate.enter_within(far()).is_some());
        drop(held);
        assert!(waiting.join().unwrap());
    });
}

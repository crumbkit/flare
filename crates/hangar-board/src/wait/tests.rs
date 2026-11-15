use super::{Gate, on_thread};
use hangar_core::contract::block_on;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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
                let _place = gate.enter();
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
        let _place = held.enter();
        panic!("the holder failed");
    })
    .join();
    let _place = gate.enter();
}

//! Rust port of `tests/test_event_queue.cpp` (6 tests).

use std::cell::Cell;
use std::rc::Rc;

use eafar::prelude::*;
use crate::suite::support::subscribe_cascade;

#[test]
fn event_queue_cascade_lightning_to_plants() {
    let mut world = FieldMap::new(16, 16);
    let mut q = EventQueue::default();
    subscribe_cascade(&mut q);

    q.push(EventType::Lightning, 5, 5, 1.0);
    check!(!q.empty());

    q.process_all(&mut world).unwrap();

    // Queue fully drained, no global loop needed.
    check!(q.empty());
    check_eq!(q.pending_count(), 0);

    // The full chain fired, in FIFO order, exactly once per type.
    let h = q.history();
    check_eq!(h.len(), 4);
    check!(h[0].type_ == EventType::Lightning);
    check!(h[1].type_ == EventType::Fire);
    check!(h[2].type_ == EventType::Smoke);
    check!(h[3].type_ == EventType::Plants);

    // Sequential numbers are monotonic (replay groundwork).
    check!(h[0].seq < h[1].seq);
    check!(h[1].seq < h[2].seq);
    check!(h[2].seq < h[3].seq);

    // Fields reflect the cascade at the strike cell.
    // Temperature: 0.50 + 0.20 = 0.70
    check_eq!(world.get(Field::Temperature, 5, 5).unwrap(), 0.70);
    // Population: -0.40 + 0.10 = -0.30 (can go negative; fuzzy layer clamps later)
    check_eq!(world.get(Field::Population, 5, 5).unwrap(), -0.30);
    // Energy: 0.05 -> 0.40 (plants restored it)
    check_eq!(world.get(Field::Energy, 5, 5).unwrap(), 0.40);
}

#[test]
fn event_cascade_is_local_to_cell() {
    let mut world = FieldMap::new(16, 16);
    let mut q = EventQueue::default();
    subscribe_cascade(&mut q);

    // Two independent strikes in different cells.
    q.push(EventType::Lightning, 2, 2, 1.0);
    q.push(EventType::Lightning, 13, 9, 1.0);
    q.process_all(&mut world).unwrap();

    // Both cells show the full cascade result...
    check_eq!(world.get(Field::Temperature, 2, 2).unwrap(), 0.70);
    check_eq!(world.get(Field::Temperature, 13, 9).unwrap(), 0.70);
    // ...and no other cell was touched (no global update).
    check_eq!(world.get(Field::Temperature, 0, 0).unwrap(), 0.0);
    check_eq!(world.get(Field::Temperature, 5, 5).unwrap(), 0.0);
    check_eq!(world.get(Field::Energy, 8, 8).unwrap(), 0.0);

    // Two interleaved chains processed in FIFO order.
    check_eq!(q.history().len(), 8);
}

#[test]
fn queue_without_handlers_drains_quietly() {
    let mut world = FieldMap::new(8, 8);
    let mut q = EventQueue::default();
    q.push(EventType::Fire, 1, 1, 0.5);
    q.push(EventType::Smoke, 1, 1, 0.5);

    q.process_all(&mut world).unwrap();

    check!(q.empty());
    check_eq!(q.processed_count(), 2);
    check_eq!(q.history().len(), 2);
    // No handler -> world untouched.
    check_eq!(world.get(Field::Temperature, 1, 1).unwrap(), 0.0);
}

#[test]
fn multiple_handlers_per_type_all_run() {
    let mut world = FieldMap::new(8, 8);
    let mut q = EventQueue::default();
    // Rc<Cell<_>> lets all three handlers share the counter with the test.
    let calls = Rc::new(Cell::new(0i32));
    for _ in 0..3 {
        let c2 = Rc::clone(&calls);
        q.subscribe(EventType::Fire, move |_, _, _| {
            c2.set(c2.get() + 1);
            Ok(())
        });
    }

    q.push(EventType::Fire, 0, 0, 1.0);
    q.process_all(&mut world).unwrap();

    check_eq!(calls.get(), 3);
}

#[test]
fn seq_numbers_are_deterministic() {
    let mut world = FieldMap::new(4, 4);
    let mut a = EventQueue::default();
    let mut b = EventQueue::default();

    a.push(EventType::Lightning, 1, 1, 1.0);
    a.push(EventType::Fire, 2, 2, 1.0);
    b.push(EventType::Lightning, 1, 1, 1.0);
    b.push(EventType::Fire, 2, 2, 1.0);

    a.process_all(&mut world).unwrap();
    b.process_all(&mut world).unwrap();

    // Identical push sequence -> identical history (sequence numbers included).
    check_eq!(a.history().len(), b.history().len());
    for (ea, eb) in a.history().iter().zip(b.history().iter()) {
        check!(ea.type_ == eb.type_);
        check!(ea.x == eb.x && ea.y == eb.y);
        check!(ea.payload == eb.payload);
        check!(ea.seq == eb.seq);
    }
}

#[test]
fn invalid_event_type_is_unrepresentable() {
    // C++ CHECK_THROWS on `EventType::_count` (an invalid type). In Rust
    // the enum has no invalid values, so `push`/`subscribe` accept every
    // variant by construction and reject nothing at runtime.
    let mut q = EventQueue::default();
    for t in EventType::ALL {
        q.push(t, 0, 0, 1.0);
        q.subscribe(t, |_, _, _| Ok(()));
    }
    check_eq!(q.pending_count(), EVENT_TYPE_COUNT);
    check_eq!(q.processed_count(), 0);
    // Names exist for every variant (C++ returned "?" only for _count).
    for t in EventType::ALL {
        check!(!event_type_name(t).is_empty());
    }
}

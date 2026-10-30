//! Rust port of `tests/test_replay.cpp` (6 tests).
//!
//! S9 spec: same seed → identical history (diff = 0).

use eafar::prelude::*;
use crate::suite::support::{make_sched, subscribe_strike_cascade};

/// A full deterministic simulation: RNG-driven lightning strikes on a world.
struct SimOutcome {
    history: Vec<Event>, // every processed event, in order
    world: FieldMap,     // final state
    processed: u64,
}

fn run_deterministic_sim(seed: u64, strikes: u32) -> SimOutcome {
    let mut world = FieldMap::new(64, 64);
    let mut q = EventQueue::default();
    subscribe_strike_cascade(&mut q);

    let mut rng = SplitMix64::new(seed);
    for _ in 0..strikes {
        let x = rng.next_u32(64);
        let y = rng.next_u32(64);
        q.push(EventType::Lightning, x, y, 1.0);
        q.process_all(&mut world).unwrap();
    }

    SimOutcome {
        history: q.history().to_vec(),
        processed: q.processed_count(),
        world,
    }
}

#[test]
fn same_seed_identical_history() {
    let a = run_deterministic_sim(12345, 1000);
    let b = run_deterministic_sim(12345, 1000);

    // Histories are identical: same length, same events, same seq numbers.
    check_eq!(a.history.len(), b.history.len());
    check_eq!(a.history.len(), 4000); // 1000 strikes * 4-event cascade
    check_eq!(a.processed, b.processed);

    for (ea, eb) in a.history.iter().zip(b.history.iter()) {
        check!(ea.type_ == eb.type_);
        check!(ea.x == eb.x);
        check!(ea.y == eb.y);
        check!(ea.payload == eb.payload);
        check!(ea.seq == eb.seq);
    }

    // Final world states are bit-exact.
    check!(a.world.bitwise_equal(&b.world));
}

#[test]
fn different_seed_different_history() {
    let a = run_deterministic_sim(1, 500);
    let b = run_deterministic_sim(2, 500);

    // Same structure, but the RNG stream differs -> strike positions differ.
    let mut any_difference = false;
    let n = a.history.len().min(b.history.len());
    for i in 0..n {
        if a.history[i].x != b.history[i].x || a.history[i].y != b.history[i].y {
            any_difference = true;
            break;
        }
    }
    check!(any_difference);
    check!(!a.world.bitwise_equal(&b.world));
}

#[test]
fn rng_sequence_is_stable_and_known() {
    // SplitMix64 canonical algorithm, seed 0. Reference values computed by
    // an independent Python implementation of the published algorithm
    // (Vigna, 2014): seed0 -> e220a8397b1dcdaf, 6e789e6aa1b965f4,
    // 6c45d188009454f, f88bb8a8724c81ec.
    let mut r = SplitMix64::new(0);
    let expected: [u64; 4] = [
        0xE220A8397B1DCDAF,
        0x6E789E6AA1B965F4,
        0x06C45D188009454F,
        0xF88BB8A8724C81EC,
    ];
    for &e in expected.iter() {
        check_eq!(r.next(), e);
    }
}

#[test]
fn rng_unit_in_unit_interval() {
    let mut r = SplitMix64::new(42);
    for _ in 0..1000 {
        let v = r.next_unit();
        check!(v >= 0.0);
        check!(v < 1.0);
    }
}

#[test]
fn rng_reproducible_after_reseed() {
    let mut r = SplitMix64::new(777);
    let first = r.next();
    r.seed(777);
    check_eq!(r.next(), first);
}

#[test]
fn snapshot_rollback_replay_same_history() {
    // Spec: snapshot -> 10000 events -> rollback -> replay produces the same
    // history as the original run.
    let mut world = FieldMap::new(64, 64);
    let mut sched = make_sched(&world);

    const STRIKES: u32 = 10000;
    let mut rng = SplitMix64::new(20260731);

    fn make_lightning_queue() -> EventQueue {
        let mut q = EventQueue::default();
        subscribe_strike_cascade(&mut q);
        q
    }

    // Original run.
    let mut q1 = make_lightning_queue();
    let pristine = WorldSnapshot::capture(&world);
    for _ in 0..STRIKES {
        let x = rng.next_u32(64);
        let y = rng.next_u32(64);
        q1.push(EventType::Lightning, x, y, 1.0);
    }
    q1.process_all(&mut world).unwrap();
    sched.tick(&mut world).unwrap();
    let original_history = q1.history().to_vec();
    let original_final = WorldSnapshot::capture(&world);
    check_eq!(original_history.len(), 40000);

    // Rollback to pristine, then replay the exact recorded event sequence.
    pristine.restore(&mut world).unwrap();
    check!(world.bitwise_equal(&FieldMap::new(64, 64)));

    let mut q2 = make_lightning_queue();
    for e in &original_history {
        // Replay only the ORIGINAL (root) events; cascades re-generate.
        if e.type_ == EventType::Lightning {
            q2.push(e.type_, e.x, e.y, e.payload);
        }
    }
    q2.process_all(&mut world).unwrap();
    sched.tick(&mut world).unwrap();

    // Replay reproduces the same final world bit-exactly.
    check!(original_final.matches(&world));

    // And the replayed history is identical to the original.
    let replay_history = q2.history();
    check_eq!(replay_history.len(), original_history.len());
    for i in 0..original_history.len() {
        check!(replay_history[i].type_ == original_history[i].type_);
        check!(replay_history[i].x == original_history[i].x);
        check!(replay_history[i].y == original_history[i].y);
        check!(replay_history[i].payload == original_history[i].payload);
        check!(replay_history[i].seq == original_history[i].seq);
    }
}

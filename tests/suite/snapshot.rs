//! Rust port of `tests/test_snapshot.cpp` (5 tests).
//!
//! S8 spec scenario: snapshot → 10000 events → rollback → bit-exact match.

use eafar::prelude::*;
use crate::suite::support::{make_sched, subscribe_strike_cascade};

#[test]
fn snapshot_rollback_after_10000_events() {
    let mut world = FieldMap::new(64, 64);
    let mut sched = make_sched(&world);

    let mut q = EventQueue::default();
    subscribe_strike_cascade(&mut q);

    // The deterministic strike pattern used by both runs.
    fn strike(qq: &mut EventQueue) {
        for i in 0..10000u32 {
            let x = i.wrapping_mul(2654435761) % 64;
            let y = i.wrapping_mul(40503) % 64;
            qq.push(EventType::Lightning, x, y, 1.0);
        }
    }

    // Run 1: from pristine world.
    strike(&mut q);
    q.process_all(&mut world).unwrap();
    sched.tick(&mut world).unwrap();
    check_eq!(q.processed_count(), 40000); // 4 events per strike
    let run1_final = WorldSnapshot::capture(&world);

    // Rollback to pristine.
    let pristine = WorldSnapshot::capture(&FieldMap::new(64, 64)); // all-zero
    let _run1_final_snap = WorldSnapshot::capture(&world); // == run1_final
    pristine.restore(&mut world).unwrap();
    check!(world.bitwise_equal(&FieldMap::new(64, 64))); // bit-exact rollback to zero

    // Run 2: same deterministic pattern from the rolled-back state.
    let mut q2 = EventQueue::default();
    subscribe_strike_cascade(&mut q2);
    strike(&mut q2);
    q2.process_all(&mut world).unwrap();
    sched.tick(&mut world).unwrap();

    // Rollback-then-redo reproduces the exact same final world (bit-exact).
    check!(run1_final.matches(&world));
    check!(!world.bitwise_equal(&FieldMap::new(64, 64))); // world is not zero
}

#[test]
fn snapshot_matches_after_restore() {
    let mut world = FieldMap::new(32, 32);
    world.set(Field::Temperature, 5, 5, 0.77).unwrap();
    world.set(Field::Corruption, 30, 30, 0.01).unwrap();

    let snap = WorldSnapshot::capture(&world);
    check!(snap.matches(&world));

    // Mutate wildly.
    world.fill(Field::Radiation, 1.0);
    world.set(Field::Temperature, 5, 5, -3.0).unwrap();
    check!(!snap.matches(&world));

    snap.restore(&mut world).unwrap();
    check!(snap.matches(&world));
    check_eq!(world.get(Field::Temperature, 5, 5).unwrap(), 0.77);
    check_eq!(world.get(Field::Corruption, 30, 30).unwrap(), 0.01);
    check_eq!(world.get(Field::Radiation, 0, 0).unwrap(), 0.0); // restored to zero
}

#[test]
fn snapshot_is_bit_exact() {
    // Bit-exact: NaN bit patterns and -0.0 survive restore byte-for-byte.
    let mut world = FieldMap::new(8, 8);
    let neg_zero = -0.0f32;
    let nan_bits = f32::from_bits(0x7FC0_0001);
    world.set(Field::Energy, 1, 1, neg_zero).unwrap();
    world.set(Field::Energy, 2, 2, nan_bits).unwrap();

    let snap = WorldSnapshot::capture(&world);
    world.set(Field::Energy, 1, 1, 123.0).unwrap();
    world.set(Field::Energy, 2, 2, 0.0).unwrap();

    snap.restore(&mut world).unwrap();

    // -0.0 == 0.0 in IEEE but must be restored as -0.0 (bit pattern).
    check!(world.get(Field::Energy, 1, 1).unwrap().is_sign_negative());
    // NaN compare with itself is false; check the bit pattern instead.
    check_eq!(
        world.get(Field::Energy, 2, 2).unwrap().to_bits(),
        0x7FC0_0001u32
    );
}

#[test]
fn snapshot_dimension_mismatch_throws() {
    let small = FieldMap::new(8, 8);
    let mut big = FieldMap::new(16, 16);
    let snap = WorldSnapshot::capture(&small);
    check_throws!(snap.restore(&mut big));
}

#[test]
fn snapshot_captures_all_six_fields() {
    let mut world = FieldMap::new(4, 4);
    for (i, &f) in Field::ALL.iter().enumerate() {
        world.set(f, 0, 0, 0.1 * (i as f32 + 1.0)).unwrap();
    }
    let snap = WorldSnapshot::capture(&world);

    for &f in Field::ALL.iter() {
        world.set(f, 0, 0, 9.0).unwrap();
    }
    snap.restore(&mut world).unwrap();

    for (i, &f) in Field::ALL.iter().enumerate() {
        check_eq!(world.get(f, 0, 0).unwrap(), 0.1 * (i as f32 + 1.0));
    }
}

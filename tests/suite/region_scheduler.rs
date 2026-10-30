//! Rust port of `tests/test_region_scheduler.cpp` (10 tests).

use std::cell::Cell;
use std::rc::Rc;

use eafar::prelude::*;
use crate::suite::support::{H, RS, W};

#[test]
fn region_geometry() {
    let s = RegionScheduler::new(W, H, RS).unwrap();
    check_eq!(s.region_size(), RS);
    check_eq!(s.regions_x(), 4);
    check_eq!(s.regions_y(), 4);
    check_eq!(s.region_count(), 16);

    // world coord (40, 70) -> region (1, 2) -> id 2*4+1 = 9
    check_eq!(s.region_x(40), 1);
    check_eq!(s.region_y(70), 2);
    check_eq!(s.region_id(40, 70), 9);
}

#[test]
fn new_scheduler_all_regions_sleep() {
    let s = RegionScheduler::new(W, H, RS).unwrap();
    check_eq!(s.active_count(), 0);
    check!(s.active_ids().is_empty());
}

#[test]
fn event_wakes_only_its_region() {
    // C++ also built an unused world here; keep the parity, silence the lint.
    let _world = FieldMap::new(W, H);
    let mut s = RegionScheduler::new(W, H, RS).unwrap();
    s.set_idle_ticks_before_sleep(5);

    // Event at (40, 40) -> region (1, 1) -> id 5
    s.wake(40, 40).unwrap();
    check_eq!(s.active_count(), 1);
    check!(s.is_active(40 / RS, 40 / RS));
    check!(s.is_active_id(5));

    // Neighbors stay asleep.
    check!(!s.is_active_id(1));
    check!(!s.is_active_id(4));
    check!(!s.is_active_id(6));
    check!(!s.is_active_id(9));
    check!(!s.is_active(0, 0));
}

#[test]
fn region_sleeps_after_idle_ticks() {
    let mut world = FieldMap::new(W, H);
    let mut s = RegionScheduler::new(W, H, RS).unwrap();
    s.set_idle_ticks_before_sleep(2);

    s.wake(40, 40).unwrap();
    check_eq!(s.active_count(), 1);

    s.tick(&mut world).unwrap(); // idle 1
    check_eq!(s.active_count(), 1);
    s.tick(&mut world).unwrap(); // idle 2 -> sleep
    check_eq!(s.active_count(), 0);
}

#[test]
fn work_runs_only_on_active_regions() {
    let mut world = FieldMap::new(W, H);
    let mut s = RegionScheduler::new(W, H, RS).unwrap();

    // Rc<Cell<_>> lets the work callback share the counters with the test.
    let work_calls = Rc::new(Cell::new(0i32));
    let last_rx = Rc::new(Cell::new(999u32));
    let last_ry = Rc::new(Cell::new(999u32));
    let (wc2, lx2, ly2) = (Rc::clone(&work_calls), Rc::clone(&last_rx), Rc::clone(&last_ry));
    s.set_region_work(move |rx, ry, _world, _sched| {
        wc2.set(wc2.get() + 1);
        lx2.set(rx);
        ly2.set(ry);
        Ok(())
    });

    s.wake(40, 40).unwrap(); // region (1,1)
    s.tick(&mut world).unwrap();

    check_eq!(work_calls.get(), 1);
    check_eq!(last_rx.get(), 1);
    check_eq!(last_ry.get(), 1);

    // Idle region produces zero work.
    s.tick(&mut world).unwrap();
    check_eq!(work_calls.get(), 1);
}

#[test]
fn work_wakes_neighbor_region() {
    let mut world = FieldMap::new(W, H);
    let mut s = RegionScheduler::new(W, H, RS).unwrap();
    s.set_idle_ticks_before_sleep(3);

    // Region (1,1) work wakes region (0,0) (e.g. event spread).
    s.set_region_work(|rx, ry, _world, sched| {
        if rx == 1 && ry == 1 {
            sched.wake_region(0, 0)?;
        }
        Ok(())
    });

    s.wake_region(1, 1).unwrap();
    s.tick(&mut world).unwrap();

    check!(s.is_active(1, 1));
    check!(s.is_active(0, 0));
    check_eq!(s.active_count(), 2);
}

#[test]
fn re_woken_region_gets_fresh_lifetime() {
    let mut world = FieldMap::new(W, H);
    let mut s = RegionScheduler::new(W, H, RS).unwrap();
    s.set_idle_ticks_before_sleep(1);

    // Work re-wakes its own region every tick (persistent activity).
    s.set_region_work(|rx, ry, _world, sched| sched.wake_region(rx, ry));

    s.wake(40, 40).unwrap();
    for _ in 0..10 {
        s.tick(&mut world).unwrap();
        check_eq!(s.active_count(), 1); // never sleeps while re-woken
    }
}

#[test]
fn sleep_is_deterministic_order() {
    // The C++ test also constructed an unused world here.
    let _world = FieldMap::new(W, H);
    let mut s = RegionScheduler::new(W, H, RS).unwrap();
    s.set_idle_ticks_before_sleep(5);

    // Wake in scrambled order; iteration order must be ascending id.
    s.wake(100, 100).unwrap(); // region (3,3) -> id 15
    s.wake(10, 10).unwrap(); // region (0,0) -> id 0
    s.wake(70, 20).unwrap(); // region (2,0) -> id 2

    let ids: Vec<u32> = s.active_ids().iter().copied().collect();
    check_eq!(ids.len(), 3);
    check_eq!(ids[0], 0);
    check_eq!(ids[1], 2);
    check_eq!(ids[2], 15);
}

#[test]
fn out_of_bounds_wake_throws() {
    let mut s = RegionScheduler::new(W, H, RS).unwrap();
    check_throws!(s.wake_region(4, 0));
    check_throws!(s.wake_region(0, 4));
    check_throws!(s.wake_region(99, 99));
}

#[test]
fn zero_region_size_throws() {
    check_throws!(RegionScheduler::new(W, H, 0));
}

#[test]
fn world_not_multiple_of_region_size() {
    // 100x100 world with 32x32 regions -> 4x4 (ceil) with partial edges.
    let s = RegionScheduler::new(100, 100, RS).unwrap();
    check_eq!(s.regions_x(), 4);
    check_eq!(s.regions_y(), 4);
    check_eq!(s.region_count(), 16);
    // Last world coord maps into the last region.
    check_eq!(s.region_x(99), 3);
    check_eq!(s.region_y(99), 3);
}

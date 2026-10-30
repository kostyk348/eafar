//! Rust port of `tests/test_automaton.cpp` (7 tests).

use std::cell::Cell;
use std::rc::Rc;

use eafar::prelude::*;
use crate::suite::support::make_sched;

#[test]
fn automaton_period_controls_cadence() {
    let mut world = FieldMap::new(64, 64);
    let mut s = make_sched(&world);
    s.wake_region(0, 0).unwrap();

    let mut m = AutomatonManager::default();
    // Rc<Cell<_>> lets the step closures share the counters with the test.
    let a = Rc::new(Cell::new(0u64));
    let b = Rc::new(Cell::new(0u64));
    let c = Rc::new(Cell::new(0u64));
    let (a2, b2, c2) = (Rc::clone(&a), Rc::clone(&b), Rc::clone(&c));
    m.add("fast", 1, move |_, _, _| {
        a2.set(a2.get() + 1);
        Ok(())
    })
    .unwrap();
    m.add("mid", 2, move |_, _, _| {
        b2.set(b2.get() + 1);
        Ok(())
    })
    .unwrap();
    m.add("slow", 3, move |_, _, _| {
        c2.set(c2.get() + 1);
        Ok(())
    })
    .unwrap();

    for _ in 0..12 {
        m.advance(&mut world, &s).unwrap();
    }

    check_eq!(a.get(), 12); // every tick
    check_eq!(b.get(), 6); // every 2nd
    check_eq!(c.get(), 4); // every 3rd
    check_eq!(m.sim_tick(), 12);
}

#[test]
fn spec_frequencies_60hz_10hz_01hz_1perhour() {
    // Base tick = 1/60 s.
    // Fire 60Hz -> period 1, Water 10Hz -> 6, Plants 0.1Hz -> 600,
    // Evolution 1/hour -> 3600 s = 216000 ticks.
    let mut world = FieldMap::new(64, 64);
    let mut s = make_sched(&world);
    s.wake_region(0, 0).unwrap();

    let mut m = AutomatonManager::default();
    m.add("Fire", 1, |_, _, _| Ok(())).unwrap();
    m.add("Water", 6, |_, _, _| Ok(())).unwrap();
    m.add("Plants", 600, |_, _, _| Ok(())).unwrap();
    m.add("Evolution", 216000, |_, _, _| Ok(())).unwrap();

    const TICKS: u64 = 216000; // one sim-hour
    for _ in 0..TICKS {
        m.advance(&mut world, &s).unwrap();
    }

    let as_ = m.automata();
    check_eq!(as_[0].name, "Fire");
    check_eq!(as_[0].executed, 216000); // 60 Hz * 3600 s
    check_eq!(as_[1].executed, 36000); // 10 Hz * 3600 s
    check_eq!(as_[2].executed, 360); // 0.1 Hz * 3600 s
    check_eq!(as_[3].executed, 1); // 1 / hour
}

#[test]
fn automaton_runs_per_active_region() {
    let mut world = FieldMap::new(64, 64);
    let mut s = make_sched(&world);
    s.wake_region(0, 0).unwrap();
    s.wake_region(1, 1).unwrap(); // two active regions

    let mut m = AutomatonManager::default();
    let visits = Rc::new(Cell::new(0i32));
    let v2 = Rc::clone(&visits);
    m.add("fire", 1, move |_, _, _| {
        v2.set(v2.get() + 1);
        Ok(())
    })
    .unwrap();

    m.advance(&mut world, &s).unwrap();

    // One step per active region per due tick.
    check_eq!(visits.get(), 2);
    check_eq!(m.at(0).unwrap().executed, 1);
}

#[test]
fn automaton_writes_fields_not_other_automata() {
    let mut world = FieldMap::new(64, 64);
    let mut s = make_sched(&world);
    s.wake_region(0, 0).unwrap();

    let mut m = AutomatonManager::default();
    // Fire raises Temperature; Water raises Humidity. No cross-references.
    m.add("Fire", 1, |_, _, w| {
        let t = w.get(Field::Temperature, 0, 0)?;
        w.set(Field::Temperature, 0, 0, t + 0.01)
    })
    .unwrap();
    m.add("Water", 1, |_, _, w| {
        let h = w.get(Field::Humidity, 0, 0)?;
        w.set(Field::Humidity, 0, 0, h + 0.02)
    })
    .unwrap();

    for _ in 0..10 {
        m.advance(&mut world, &s).unwrap();
    }

    // Accumulated additions drift from the decimal literal; compare with eps.
    check_near!(world.get(Field::Temperature, 0, 0).unwrap(), 0.10, 1e-6);
    check_near!(world.get(Field::Humidity, 0, 0).unwrap(), 0.20, 1e-6);
    check_eq!(world.get(Field::Energy, 0, 0).unwrap(), 0.0); // untouched
}

#[test]
fn automaton_never_runs_in_sleeping_regions() {
    let mut world = FieldMap::new(64, 64);
    let mut s = RegionScheduler::new(64, 64, 32).unwrap();
    s.set_idle_ticks_before_sleep(1); // sleep after one tick
    s.wake_region(0, 0).unwrap();

    let mut m = AutomatonManager::default();
    let visits = Rc::new(Cell::new(0i32));
    let v2 = Rc::clone(&visits);
    m.add("fire", 1, move |_, _, _| {
        v2.set(v2.get() + 1);
        Ok(())
    })
    .unwrap();

    // Runtime loop: cadence (advance) + region aging (tick).
    m.advance(&mut world, &s).unwrap();
    s.tick(&mut world).unwrap(); // ages region 0,0 -> sleeps
    check_eq!(visits.get(), 1);

    m.advance(&mut world, &s).unwrap();
    s.tick(&mut world).unwrap();
    check_eq!(visits.get(), 1); // no active regions -> no visits
}

#[test]
fn automaton_deterministic_across_runs() {
    // Same config, same tick count -> identical counters and world state.
    fn run_once() -> (u64, u64, f32) {
        let mut world = FieldMap::new(64, 64);
        let mut s = make_sched(&world);
        s.wake_region(1, 1).unwrap();

        let mut m = AutomatonManager::default();
        m.add("Fire", 1, |_, _, w| {
            let t = w.get(Field::Temperature, 0, 0)?;
            w.set(Field::Temperature, 0, 0, t + 0.5)
        })
        .unwrap();
        m.add("Water", 7, |_, _, w| {
            let h = w.get(Field::Humidity, 0, 0)?;
            w.set(Field::Humidity, 0, 0, h + 0.1)
        })
        .unwrap();

        for _ in 0..100 {
            m.advance(&mut world, &s).unwrap();
        }

        (
            m.at(0).unwrap().executed,
            m.at(1).unwrap().executed,
            world.get(Field::Temperature, 0, 0).unwrap(),
        )
    }

    let (f1, w1, t1) = run_once();
    let (f2, w2, t2) = run_once();

    check_eq!(f1, f2);
    check_eq!(w1, w2);
    check_eq!(t1, t2);
}

#[test]
fn zero_period_throws() {
    let mut m = AutomatonManager::default();
    check_throws!(m.add("bad", 0, |_, _, _| Ok(())));
    check_eq!(m.count(), 0);
}

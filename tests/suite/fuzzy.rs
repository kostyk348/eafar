//! Rust port of `tests/test_fuzzy.cpp` (10 tests).

use std::cell::RefCell;
use std::rc::Rc;

use eafar::prelude::*;
use crate::suite::support::make_sched;

#[test]
fn step_membership() {
    let s = mf::step(0.8);
    check_eq!(s(0.0), 0.0);
    check_eq!(s(0.79), 0.0);
    check_eq!(s(0.8), 1.0);
    check_eq!(s(1.0), 1.0);
}

#[test]
fn ramp_membership() {
    let r = mf::ramp(0.2, 0.8);
    check_eq!(r(0.1), 0.0);
    check_eq!(r(0.5), 0.5);
    check_eq!(r(0.8), 1.0);
    check_eq!(r(1.0), 1.0);
}

#[test]
fn triangle_membership() {
    let t = mf::triangle(0.5, 0.2);
    check_eq!(t(0.5), 1.0);
    // At the exact edge |d| == width float rounding gives ~1e-8, not exactly 0.
    check_near!(t(0.3), 0.0, 1e-6);
    check_near!(t(0.4), 0.5, 1e-6);
    check_near!(t(0.6), 0.5, 1e-6);
    check_near!(t(0.0), 0.0, 1e-6);
}

#[test]
fn bell_membership() {
    let b = mf::bell(0.5, 0.1);
    check_near!(b(0.5), 1.0, 1e-6);
    check_near!(b(0.6), 0.5, 1e-6);
    check_near!(b(0.4), 0.5, 1e-6);
}

#[test]
fn registry_define_apply() {
    let mut reg = FuzzyRegistry::default();
    reg.define("ignition", mf::step(0.8));
    check!(reg.has("ignition"));
    check_eq!(reg.count(), 1);

    check_eq!(reg.apply("ignition", 0.5).unwrap(), 0.0);
    check_eq!(reg.apply("ignition", 0.9).unwrap(), 1.0);
}

#[test]
fn registry_unknown_name_throws() {
    let mut reg = FuzzyRegistry::default();
    reg.define("ignition", mf::step(0.8));
    check_throws!(reg.apply("nope", 0.5));
}

#[test]
fn registry_define_swaps_behavior() {
    // The KEY S5 property: automaton references membership by NAME;
    // swapping the fn changes behavior without editing the automaton.
    // (RefCell mirrors the C++ ability to mutate a registry that is also
    // borrowed by closures.)
    let reg = RefCell::new(FuzzyRegistry::default());
    reg.borrow_mut().define("hot", mf::step(0.9)); // sharp threshold at 0.9

    // Automaton: if "hot" membership of Temperature >= 0.5 -> set Energy 1.
    let ignition_step = |w: &mut FieldMap| {
        let reg = reg.borrow();
        let hot = reg
            .apply("hot", w.get(Field::Temperature, 0, 0).unwrap())
            .unwrap();
        if hot >= 0.5 {
            w.set(Field::Energy, 0, 0, 1.0).unwrap();
        }
    };

    let mut world = FieldMap::new(4, 4);

    // Case 1: step(0.9) — 0.85 is not hot.
    world.set(Field::Temperature, 0, 0, 0.85).unwrap();
    ignition_step(&mut world);
    check_eq!(world.get(Field::Energy, 0, 0).unwrap(), 0.0);

    // Swap the membership function. SAME automaton code.
    reg.borrow_mut().define("hot", mf::ramp(0.5, 0.8));
    world.set(Field::Energy, 0, 0, 0.0).unwrap();
    ignition_step(&mut world);
    // 0.85 is above hi(0.8) -> membership 1 -> ignites.
    check_eq!(world.get(Field::Energy, 0, 0).unwrap(), 1.0);
}

#[test]
fn registry_swaps_without_editing_automaton_in_pipeline() {
    // Integration: a Fire automaton reads "flammable" membership; the
    // registry is swapped mid-run and the same automaton reacts differently.
    let reg = Rc::new(RefCell::new(FuzzyRegistry::default()));
    reg.borrow_mut().define("flammable", mf::step(0.7)); // value >= 0.7 -> flammable

    let mut world = FieldMap::new(64, 64);
    let mut s = make_sched(&world);
    s.wake_region(0, 0).unwrap();

    let mut m = AutomatonManager::default();
    let reg_for_step = Rc::clone(&reg);
    m.add("Fire", 1, move |_, _, w| {
        let reg = reg_for_step.borrow();
        let f = reg
            .apply("flammable", w.get(Field::Humidity, 0, 0).unwrap())
            .unwrap();
        // Flammable -> fire consumes Energy.
        if f >= 0.5 {
            w.set(Field::Energy, 0, 0, 0.0)?;
        }
        Ok(())
    })
    .unwrap();

    // Wet world (humidity 0.9): flammable under step(0.7) -> energy consumed.
    world.set(Field::Humidity, 0, 0, 0.9).unwrap();
    world.set(Field::Energy, 0, 0, 1.0).unwrap();
    m.advance(&mut world, &s).unwrap();
    check_eq!(world.get(Field::Energy, 0, 0).unwrap(), 0.0);

    // Dry world (humidity 0.2): NOT flammable under step(0.7) -> untouched.
    world.set(Field::Humidity, 0, 0, 0.2).unwrap();
    world.set(Field::Energy, 0, 0, 1.0).unwrap();
    m.advance(&mut world, &s).unwrap();
    check_eq!(world.get(Field::Energy, 0, 0).unwrap(), 1.0);

    // Swap to "dry = flammable" semantics: SAME automaton now consumes.
    reg.borrow_mut()
        .define("flammable", |v: f32| if v < 0.8 { 1.0 } else { 0.0 });
    m.advance(&mut world, &s).unwrap();
    check_eq!(world.get(Field::Energy, 0, 0).unwrap(), 0.0);
}

#[test]
fn clamp01_bounds() {
    check_eq!(clamp01(-1.0), 0.0);
    check_eq!(clamp01(0.5), 0.5);
    check_eq!(clamp01(1.5), 1.0);
}

#[test]
fn null_membership_is_unrepresentable() {
    // C++ CHECK_THROWS on `define(name, nullptr)`. In Rust an
    // `Arc<dyn Fn>` has no null state, so `define` is total by
    // construction; verify a defined membership is immediately usable.
    let mut reg = FuzzyRegistry::default();
    reg.define("never_null", mf::step(1.0));
    check!(reg.has("never_null"));
    check_eq!(reg.apply("never_null", 1.0).unwrap(), 1.0);
    check_eq!(reg.apply("never_null", 0.5).unwrap(), 0.0);
}

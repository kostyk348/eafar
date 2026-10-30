//! Shared support for the contract test suite — Rust port of
//! `tests/test_runner.hpp` (CHECK/CHECK_EQ/CHECK_NEAR/CHECK_THROWS macros)
//! plus the shared simulation fixtures used by several C++ test files.

/// C++ `CHECK(cond)` — panic with the original expression text on failure.
macro_rules! check {
    ($cond:expr) => {
        assert!(
            $cond,
            "CHECK failed: {} ({}:{})",
            stringify!($cond),
            file!(),
            line!()
        )
    };
}

/// C++ `CHECK_EQ(a, b)`.
macro_rules! check_eq {
    ($a:expr, $b:expr) => {
        assert_eq!(
            $a, $b,
            "CHECK_EQ failed: {} == {} ({}:{})",
            stringify!($a),
            stringify!($b),
            file!(),
            line!()
        )
    };
}

/// C++ `CHECK_NEAR(a, b, eps)` — float compare with tolerance
/// (accumulation drift aware).
macro_rules! check_near {
    ($a:expr, $b:expr, $eps:expr) => {{
        let (va, vb): (f32, f32) = ($a, $b);
        let eps: f32 = $eps;
        let delta = if va > vb { va - vb } else { vb - va };
        assert!(
            delta <= eps,
            "CHECK_NEAR failed: {} ~= {} (va={va}, vb={vb}, delta={delta} > eps={eps}) at {}:{}",
            stringify!($a),
            stringify!($b),
            file!(),
            line!()
        );
    }};
}

/// C++ `CHECK_THROWS(expr)` — the C++ suite asserted exceptions; the Rust
/// API reports failures as `Result`, so this asserts the operation errors.
macro_rules! check_throws {
    ($e:expr) => {{
        let r = $e;
        assert!(
            r.is_err(),
            "CHECK_THROWS failed: `{}` completed successfully ({}:{})",
            stringify!($e),
            file!(),
            line!()
        );
    }};
}

use eafar::prelude::*;

/// 128x128 world, 32x32 regions -> 4x4 = 16 regions (constants shared with
/// the C++ region-scheduler tests).
pub const W: u32 = 128;
pub const H: u32 = 128;
pub const RS: u32 = 32;

/// Scheduler for a world with 32x32 regions that never sleeps during tests
/// (C++ `make_sched` in test_automaton.cpp).
pub fn make_sched(world: &FieldMap) -> RegionScheduler {
    let mut s = RegionScheduler::new(world.width(), world.height(), 32).unwrap();
    s.set_idle_ticks_before_sleep(1 << 30); // never sleep during test
    s
}

// ---------------------------------------------------------------------------
// The spec cascade: Lightning -> Fire -> Smoke -> Plants (test_event_queue).
// Each handler writes a field (S1 coupling) and pushes the next event.
// No handler references any other handler — only fields and the queue.
// Moderate values so the arithmetic stays in [0,1] for a single strike.
// ---------------------------------------------------------------------------
pub fn subscribe_cascade(q: &mut EventQueue) {
    q.subscribe(EventType::Lightning, |e, w, q| {
        // Lightning deposits heat at its cell...
        let t = w.get(Field::Temperature, e.x, e.y)?;
        w.set(Field::Temperature, e.x, e.y, t + 0.50)?;
        // ... and ignites the cell.
        q.push(EventType::Fire, e.x, e.y, 0.90);
        Ok(())
    });
    q.subscribe(EventType::Fire, |e, w, q| {
        // Fire burns fuel (Energy) and keeps raising Temperature.
        let t = w.get(Field::Temperature, e.x, e.y)?;
        w.set(Field::Temperature, e.x, e.y, t + 0.20)?;
        w.set(Field::Energy, e.x, e.y, 0.05)?; // fuel mostly consumed
        // Smoke carries the burn upward/outward.
        q.push(EventType::Smoke, e.x, e.y, 0.70);
        Ok(())
    });
    q.subscribe(EventType::Smoke, |e, w, q| {
        // Smoke chokes Population at the cell.
        let p = w.get(Field::Population, e.x, e.y)?;
        w.set(Field::Population, e.x, e.y, p - 0.40)?;
        // After the smoke, plants regrow.
        q.push(EventType::Plants, e.x, e.y, 0.30);
        Ok(())
    });
    q.subscribe(EventType::Plants, |e, w, _q| {
        // Plants recover Population and restore Energy.
        let p = w.get(Field::Population, e.x, e.y)?;
        w.set(Field::Population, e.x, e.y, p + 0.10)?;
        w.set(Field::Energy, e.x, e.y, 0.40)?;
        Ok(())
    });
}

// ---------------------------------------------------------------------------
// The strike cascade used by the snapshot/replay tests: same chain, but the
// Fire handler does not bump Temperature.
// ---------------------------------------------------------------------------
pub fn subscribe_strike_cascade(q: &mut EventQueue) {
    q.subscribe(EventType::Lightning, |e, w, q| {
        let t = w.get(Field::Temperature, e.x, e.y)?;
        w.set(Field::Temperature, e.x, e.y, t + 0.50)?;
        q.push(EventType::Fire, e.x, e.y, 0.90);
        Ok(())
    });
    q.subscribe(EventType::Fire, |e, w, q| {
        w.set(Field::Energy, e.x, e.y, 0.05)?;
        q.push(EventType::Smoke, e.x, e.y, 0.70);
        Ok(())
    });
    q.subscribe(EventType::Smoke, |e, w, q| {
        let p = w.get(Field::Population, e.x, e.y)?;
        w.set(Field::Population, e.x, e.y, p - 0.40)?;
        q.push(EventType::Plants, e.x, e.y, 0.30);
        Ok(())
    });
    q.subscribe(EventType::Plants, |e, w, _q| {
        w.set(Field::Energy, e.x, e.y, 0.40)?;
        Ok(())
    });
}

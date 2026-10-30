//! S4: Asynchronous Automata.
//!
//! C++ lineage: `include/eafar/automaton.hpp` + `src/automaton.cpp`.
//!
//! Every automaton lives on its own clock: it runs every `period_ticks`
//! base simulation ticks. Fire at 60Hz → period 1, Water at 10Hz → 6,
//! Plants at 0.1Hz → 600, Evolution at 1/hour → 216000 (at 60 ticks/sec).
//!
//! An automaton step is executed per ACTIVE region (region locality from
//! S3), and reads/writes only fields (coupling rule from S1/S6) — it never
//! references other automata. All iteration order is fixed (registration
//! order, ascending region id) ⇒ deterministic.

use std::sync::Arc;

use crate::error::{Error, Result};
use crate::field_map::FieldMap;
use crate::region_scheduler::RegionScheduler;

/// One automaton step: executed per active region (C++ `Step`).
pub type Step = Arc<dyn Fn(u32, u32, &mut FieldMap) -> Result<()>>;

/// A registered automaton: name, cadence and step callback.
///
/// C++ lineage: `struct Automaton`.
#[derive(Clone)]
pub struct Automaton {
    /// Registration name (informational).
    pub name: String,
    /// Run every N base ticks (must be >= 1).
    pub period: u64,
    /// Times the step ran (due ticks).
    pub executed: u64,
    /// The step callback.
    pub step: Step,
}

/// Registry and driver of automata (C++ `class AutomatonManager`).
#[derive(Default)]
pub struct AutomatonManager {
    automata: Vec<Automaton>,
    sim_tick: u64,
}

impl AutomatonManager {
    /// Register an automaton. `period` must be >= 1 (C++ `add`; a zero
    /// period is an error, mirroring the C++ `std::invalid_argument`).
    ///
    /// Returns the registration index.
    pub fn add(
        &mut self,
        name: impl Into<String>,
        period: u64,
        step: impl Fn(u32, u32, &mut FieldMap) -> Result<()> + 'static,
    ) -> Result<usize> {
        if period == 0 {
            return Err(Error::invalid_argument(
                "eafar: automaton period must be >= 1",
            ));
        }
        self.automata.push(Automaton {
            name: name.into(),
            period,
            executed: 0,
            step: Arc::new(step),
        });
        Ok(self.automata.len() - 1)
    }

    /// Advance the simulation by one base tick. Every due automaton runs
    /// its step over every ACTIVE region (ascending region id).
    /// Deterministic (C++ `advance`).
    pub fn advance(&mut self, world: &mut FieldMap, sched: &RegionScheduler) -> Result<()> {
        self.sim_tick += 1;
        for a in &mut self.automata {
            if self.sim_tick % a.period != 0 {
                continue;
            }
            // Per active region, ascending id (deterministic).
            let step = a.step.clone();
            for &id in sched.active_ids() {
                let rx = id % sched.regions_x();
                let ry = id / sched.regions_x();
                step(rx, ry, world)?;
            }
            a.executed += 1;
        }
        Ok(())
    }

    /// Current base tick counter (C++ `sim_tick`).
    pub fn sim_tick(&self) -> u64 {
        self.sim_tick
    }

    /// Number of registered automata (C++ `count`).
    pub fn count(&self) -> usize {
        self.automata.len()
    }

    /// Access an automaton by registration index (C++ `at`, which threw on
    /// out-of-range; the Rust port returns an [`Option`]).
    pub fn at(&self, idx: usize) -> Option<&Automaton> {
        self.automata.get(idx)
    }

    /// All automata in registration order (C++ `automata`).
    pub fn automata(&self) -> &[Automaton] {
        &self.automata
    }
}

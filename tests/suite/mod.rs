//! Contract test suite — Rust port of the C++ `tests/` directory
//! (`tests/*.cpp` + `tests/test_runner.hpp`), extended with the audit
//! determinism/replay property tests (`determinism`).
//!
//! The C++ custom registry/runner maps to cargo's built-in test harness:
//! each `TEST(name)` became a `#[test] fn name()`, each translation unit a
//! module, and the CHECK macros live in [`support`].

#[macro_use]
pub mod support;

pub mod automaton;
pub mod dependency;
pub mod determinism;
pub mod diffusion;
pub mod event_queue;
pub mod field_map;
pub mod fuzzy;
pub mod region_scheduler;
pub mod replay;
pub mod snapshot;
pub mod visualizer;

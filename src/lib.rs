//! # eafar — Event-Driven Asynchronous Fuzzy Automata Runtime
//!
//! Pure-Rust port of the EAFAR C++23 library (modules S1–S10). EAFAR is a
//! runtime for large-scale fuzzy cellular simulations built on ten ideas:
//!
//! | Module | Subsystem | C++ lineage |
//! |--------|-----------|-------------|
//! | [`field`] | semantic field types | `include/eafar/field.hpp` |
//! | [`field_map`] | S1: spatial state storage (SoA) | `include/eafar/field_map.hpp` |
//! | [`event`] | event types and payload | `include/eafar/event.hpp` |
//! | [`event_queue`] | S2: event queue + cascades | `include/eafar/event_queue.hpp` |
//! | [`region_scheduler`] | S3: region scheduler (locality) | `include/eafar/region_scheduler.hpp` |
//! | [`automaton`] | S4: asynchronous automata | `include/eafar/automaton.hpp` |
//! | [`fuzzy`] | S5: fuzzy state system | `include/eafar/fuzzy.hpp` |
//! | [`dependency`] | S6: dependency graph | `include/eafar/dependency.hpp` |
//! | [`diffusion`] | S7: diffusion engine | `include/eafar/diffusion.hpp` |
//! | [`snapshot`] | S8: snapshot / rollback | `include/eafar/snapshot.hpp` |
//! | [`rng`] | S9: deterministic PRNG | `include/eafar/rng.hpp` |
//! | [`visualizer`] | S10: PNG/CSV export | `include/eafar/visualizer.hpp` |
//!
//! ## Porting rules
//!
//! * **Determinism above all.** Iteration is always in ascending id order
//!   (sorted sets, never hash order), the PRNG is a fixed SplitMix64, and
//!   float ops run in a fixed order. Same seed → same world, bit-exact.
//! * **Snapshots are bit-exact.** Capture/restore preserve every float bit
//!   pattern (including NaN payloads and `-0.0`).
//! * **Errors as values.** C++ exceptions map to [`error::Result`]; handlers,
//!   automaton steps and region work return `Result` and the runtime
//!   propagates the first error, exactly like the C++ exception flow.
//! * **Zero dependencies.** No third-party crates, not even for tests.

#![forbid(unsafe_code)]

pub mod automaton;
pub mod dependency;
pub mod diffusion;
pub mod error;
pub mod event;
pub mod event_queue;
pub mod field;
pub mod field_map;
pub mod fuzzy;
pub mod prelude;
pub mod region_scheduler;
pub mod rng;
pub mod snapshot;
pub mod visualizer;

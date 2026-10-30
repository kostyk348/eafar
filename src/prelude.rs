//! Re-export of the whole public API in one `use eafar::prelude::*;`.
//!
//! Mirrors `using namespace eafar;` in the C++ examples and tests.

pub use crate::automaton::{Automaton, AutomatonManager, Step};
pub use crate::dependency::{Access, DepEdge, DependencyGraph, FieldAccess};
pub use crate::diffusion::{DiffusionEngine, DiffusionRule, Kernel};
pub use crate::error::{Error, Result};
pub use crate::event::{event_type_name, Event, EventType, EVENT_TYPE_COUNT};
pub use crate::event_queue::{EventQueue, EventHandler};
pub use crate::field::{field_name, Field, FIELD_COUNT};
pub use crate::field_map::FieldMap;
pub use crate::fuzzy::{clamp01, mf, FuzzyRegistry, IntoMembershipFn, MembershipFn};
pub use crate::region_scheduler::{RegionScheduler, RegionWork};
pub use crate::rng::SplitMix64;
pub use crate::snapshot::WorldSnapshot;
pub use crate::visualizer::Visualizer;

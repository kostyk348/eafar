//! S5: Fuzzy State System.
//!
//! C++ lineage: `include/eafar/fuzzy.hpp`.
//!
//! No bools — everything is a float membership in `[0..1]`. A membership
//! function maps a raw field value to a degree of "the predicate holds"
//! (e.g. `ignition(x)` = how flammable a Temperature reading is).
//!
//! Membership functions are pluggable and swapped WITHOUT editing the
//! automaton that consumes them: automata depend on a named membership,
//! the runtime resolves it.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::{Error, Result};

/// A membership function: raw value → membership degree in `[0,1]`
/// (C++ `MembershipFn`).
pub type MembershipFn = Arc<dyn Fn(f32) -> f32>;

/// Anything usable as a membership function in [`FuzzyRegistry::define`].
///
/// C++ lineage: the C++ `define` accepted any `MembershipFn`
/// (`std::function`), which transparently swallowed raw lambdas and
/// shared callables alike. Rust has no such implicit conversion, so this
/// trait restores the same call-site ergonomics: pass a plain closure or
/// an already-shared [`MembershipFn`].
pub trait IntoMembershipFn {
    /// Convert into the stored shared membership function.
    fn into_membership_fn(self) -> MembershipFn;
}

impl<F: Fn(f32) -> f32 + 'static> IntoMembershipFn for F {
    fn into_membership_fn(self) -> MembershipFn {
        Arc::new(self)
    }
}

impl IntoMembershipFn for MembershipFn {
    fn into_membership_fn(self) -> MembershipFn {
        self
    }
}

/// Common built-in shapes (C++ `namespace mf`).
pub mod mf {
    use std::sync::Arc;

    use super::MembershipFn;

    /// Step: 0 below `threshold`, 1 at/above. Sharp, fuzzy-free.
    pub fn step(threshold: f32) -> MembershipFn {
        Arc::new(move |v: f32| if v >= threshold { 1.0 } else { 0.0 })
    }

    /// Ramp: 0 at/below `lo`, 1 at/above `hi`, linear between.
    pub fn ramp(lo: f32, hi: f32) -> MembershipFn {
        Arc::new(move |v: f32| {
            if v <= lo {
                0.0
            } else if v >= hi {
                1.0
            } else {
                (v - lo) / (hi - lo)
            }
        })
    }

    /// Triangle: rises to 1 at `center`, 0 at `center ± width`.
    pub fn triangle(center: f32, width: f32) -> MembershipFn {
        Arc::new(move |v: f32| {
            let d = if v > center { v - center } else { center - v };
            if d >= width {
                0.0
            } else {
                1.0 - d / width
            }
        })
    }

    /// Gaussian-ish bell (deterministic, no libm needed).
    pub fn bell(center: f32, spread: f32) -> MembershipFn {
        Arc::new(move |v: f32| {
            let d = if v > center { v - center } else { center - v };
            let r = d / spread;
            1.0 / (1.0 + r * r)
        })
    }
}

/// Named membership registry. Automata reference memberships by name, so
/// swapping the function changes behavior without touching automata.
///
/// C++ lineage: `class FuzzyRegistry`. (The C++ null-function check is not
/// needed in Rust: an `Arc<dyn Fn>` cannot be null, so `define` is total.)
pub struct FuzzyRegistry {
    fns: HashMap<String, MembershipFn>,
}

impl Default for FuzzyRegistry {
    fn default() -> Self {
        FuzzyRegistry {
            fns: HashMap::new(),
        }
    }
}

impl FuzzyRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Define (or replace) a named membership (C++ `define`). Accepts a
    /// plain closure or a shared [`MembershipFn`]. (The C++ null-function
    /// check is not needed in Rust: an `Arc<dyn Fn>` cannot be null, so
    /// `define` is total.)
    pub fn define(&mut self, name: impl Into<String>, f: impl IntoMembershipFn) {
        self.fns.insert(name.into(), f.into_membership_fn());
    }

    /// Apply a named membership to a raw value. Errors when the name is
    /// unknown (C++ threw `std::out_of_range`).
    pub fn apply(&self, name: &str, raw: f32) -> Result<f32> {
        match self.fns.get(name) {
            Some(f) => Ok(f(raw)),
            None => Err(Error::not_found(&format!(
                "eafar: unknown membership '{name}'"
            ))),
        }
    }

    /// True when a membership with this name exists (C++ `has`).
    pub fn has(&self, name: &str) -> bool {
        self.fns.contains_key(name)
    }

    /// Number of defined memberships (C++ `count`).
    pub fn count(&self) -> usize {
        self.fns.len()
    }
}

/// Convenience: clamp a raw value into the fuzzy `[0,1]` domain
/// (C++ `clamp01`).
pub fn clamp01(v: f32) -> f32 {
    if v < 0.0 {
        0.0
    } else if v > 1.0 {
        1.0
    } else {
        v
    }
}

//! S8: Snapshot / Rollback.
//!
//! C++ lineage: `include/eafar/snapshot.hpp`.
//!
//! A snapshot is a copy of the entire world state (every field array).
//! Because the world is SoA, a snapshot is just N contiguous arrays —
//! capture and restore are memcpy-equivalent, fast and bit-exact.
//!
//! Usage: snapshot → run 10000 events → restore → the world is
//! bit-identical to the snapshot. This powers debugging, "what if"
//! exploration, and (with S9) rollback-then-replay.
//!
//! Bit-exactness: values are compared and copied through
//! `f32::to_bits`/`from_bits` semantics (plain `Copy` of `f32` in Rust is
//! a bit copy), so NaN payloads and `-0.0` survive round-trips — the same
//! guarantee the C++ `memcmp` implementation provided.

use crate::error::{Error, Result};
use crate::field::{Field, FIELD_COUNT};
use crate::field_map::FieldMap;

/// Full-world capture (C++ `class WorldSnapshot`).
#[derive(Debug, Clone)]
pub struct WorldSnapshot {
    width: u32,
    height: u32,
    fields: [Vec<f32>; FIELD_COUNT],
}

impl Default for WorldSnapshot {
    fn default() -> Self {
        WorldSnapshot {
            width: 0,
            height: 0,
            fields: std::array::from_fn(|_| Vec::new()),
        }
    }
}

impl WorldSnapshot {
    /// Capture the entire world (C++ `capture`).
    pub fn capture(world: &FieldMap) -> Self {
        WorldSnapshot {
            width: world.width(),
            height: world.height(),
            fields: std::array::from_fn(|i| world.data(Field::ALL[i]).to_vec()),
        }
    }

    /// Restore into an existing world. The world must have the same
    /// dimensions (checked); arrays are copied bit-exactly (C++ `restore`,
    /// which threw on mismatch).
    pub fn restore(&self, world: &mut FieldMap) -> Result<()> {
        if world.width() != self.width || world.height() != self.height {
            return Err(Error::invalid_argument(
                "eafar: snapshot dimensions do not match target world",
            ));
        }
        for i in 0..FIELD_COUNT {
            let f = Field::ALL[i];
            world.data_mut(f).copy_from_slice(&self.fields[i]);
        }
        Ok(())
    }

    /// Bit-exact comparison of the snapshot against a world
    /// (C++ `matches`).
    pub fn matches(&self, world: &FieldMap) -> bool {
        if world.width() != self.width || world.height() != self.height {
            return false;
        }
        for i in 0..FIELD_COUNT {
            let f = Field::ALL[i];
            let a = world.data(f);
            let b = &self.fields[i];
            if a.len() != b.len() {
                return false;
            }
            for (x, y) in a.iter().zip(b.iter()) {
                if x.to_bits() != y.to_bits() {
                    return false;
                }
            }
        }
        true
    }

    /// Snapshot width (C++ `width`).
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Snapshot height (C++ `height`).
    pub fn height(&self) -> u32 {
        self.height
    }
}

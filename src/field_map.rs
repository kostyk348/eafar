//! S1: Spatial State Storage.
//!
//! C++ lineage: `include/eafar/field_map.hpp` + `src/field_map.cpp`.
//!
//! The world is stored as SoA (Struct-of-Arrays) fields: one contiguous
//! float array per semantic field. There are NO per-cell objects — a cell
//! is just an index shared by all fields. This is what makes snapshots
//! (memcpy of N arrays), parallelism and field-level diffing cheap.

use crate::error::{Error, Result};
use crate::field::{Field, FIELD_COUNT};

/// The world: `width x height` cells, `FIELD_COUNT` parallel float arrays.
///
/// C++ lineage: `class FieldMap`.
#[derive(Debug, Clone, Default)]
pub struct FieldMap {
    width: u32,
    height: u32,
    fields: [Vec<f32>; FIELD_COUNT],
}

impl FieldMap {
    /// Create a zeroed world of the given dimensions
    /// (C++ `FieldMap(width, height)`).
    pub fn new(width: u32, height: u32) -> Self {
        let n = width as usize * height as usize;
        FieldMap {
            width,
            height,
            fields: std::array::from_fn(|_| vec![0.0; n]),
        }
    }

    /// World width in cells.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// World height in cells.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Total cell count (`width * height`).
    pub fn cell_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    /// True if the coordinate lies inside the world.
    pub fn in_bounds(&self, x: u32, y: u32) -> bool {
        x < self.width && y < self.height
    }

    /// Row-major linear index of a cell (C++ `index`).
    pub fn index(&self, x: u32, y: u32) -> usize {
        y as usize * self.width as usize + x as usize
    }

    /// Checked cell read. Errors with [`Error::OutOfBounds`] when
    /// `(x, y)` lies outside the world (C++ threw `std::out_of_range`).
    pub fn get(&self, f: Field, x: u32, y: u32) -> Result<f32> {
        if !self.in_bounds(x, y) {
            return Err(Error::out_of_bounds("eafar: FieldMap::get out of bounds"));
        }
        Ok(self.fields[f as usize][self.index(x, y)])
    }

    /// Checked cell write. Errors with [`Error::OutOfBounds`] when
    /// `(x, y)` lies outside the world (C++ threw `std::out_of_range`).
    pub fn set(&mut self, f: Field, x: u32, y: u32, value: f32) -> Result<()> {
        if !self.in_bounds(x, y) {
            return Err(Error::out_of_bounds("eafar: FieldMap::set out of bounds"));
        }
        let idx = self.index(x, y);
        self.fields[f as usize][idx] = value;
        Ok(())
    }

    /// Raw SoA slice of a field, for bulk / diff / snapshot paths
    /// (C++ `const float* data(Field)`).
    pub fn data(&self, f: Field) -> &[f32] {
        &self.fields[f as usize]
    }

    /// Mutable raw SoA slice of a field (C++ `float* data(Field)`).
    pub fn data_mut(&mut self, f: Field) -> &mut [f32] {
        &mut self.fields[f as usize]
    }

    /// Set every cell of one field to `value`.
    pub fn fill(&mut self, f: Field, value: f32) {
        for v in self.fields[f as usize].iter_mut() {
            *v = value;
        }
    }

    /// Zero every field.
    pub fn clear(&mut self) {
        for arr in self.fields.iter_mut() {
            for v in arr.iter_mut() {
                *v = 0.0;
            }
        }
    }

    /// Deterministic equality: bit-exact compare of every field array.
    ///
    /// Bit comparison (not `==`) so that NaN payloads and `-0.0` count as
    /// differences, exactly like the C++ `memcmp`-based implementation.
    pub fn bitwise_equal(&self, other: &FieldMap) -> bool {
        if self.width != other.width || self.height != other.height {
            return false;
        }
        for (a, b) in self.fields.iter().zip(other.fields.iter()) {
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
}

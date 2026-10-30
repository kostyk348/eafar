//! S7: Diffusion Engine.
//!
//! C++ lineage: `include/eafar/diffusion.hpp` + `src/diffusion.cpp`.
//!
//! ONE infrastructure for every spreading process (heat, water, radiation,
//! disease, smell, sound). Only the RULE differs: stencil kernel, transfer
//! rate, decay, ambient. A diffusion step runs only inside ACTIVE regions
//! (S3 locality) but reads neighbors across region boundaries.
//!
//! Determinism: row-major iteration, scratch-buffer commit (no in-place
//! read-after-write), fixed float ops — same order every run.

use std::collections::BTreeSet;

use crate::error::{Error, Result};
use crate::field::Field;
use crate::field_map::FieldMap;

/// Stencil kernel (C++ `enum class Kernel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kernel {
    /// 4-neighborhood (up/down/left/right).
    #[default]
    VonNeumann,
    /// 8-neighborhood (adds the diagonals).
    Moore,
}

/// Parameters of one diffusion process (C++ `struct DiffusionRule`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffusionRule {
    /// Stencil shape.
    pub kernel: Kernel,
    /// Fraction of neighbor mean transferred per step, `[0,1]`.
    pub rate: f32,
    /// Extra per-step decay toward 0 (e.g. radiation).
    pub decay: f32,
    /// If > 0, relax toward this value instead of neighbors.
    pub ambient: f32,
}

impl Default for DiffusionRule {
    fn default() -> Self {
        DiffusionRule {
            kernel: Kernel::VonNeumann,
            rate: 0.1,
            decay: 0.0,
            ambient: 0.0,
        }
    }
}

/// Region-relative offsets of the 4/8 neighbors (same order as C++).
const N4: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const N8: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// Applies diffusion rules to fields, region by region.
///
/// C++ lineage: `class DiffusionEngine`.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiffusionEngine;

impl DiffusionEngine {
    /// Per-cell neighbor mean, honoring world edges. Returns
    /// `(mean, neighbor count)`.
    ///
    /// `self_value` is the cell's own value, used as the mean when the
    /// cell has no in-bounds neighbors — the same fallback the C++ helper
    /// computed via `world.get(field, x, y)`.
    fn neighbor_mean(
        &self,
        world: &FieldMap,
        field: Field,
        x: u32,
        y: u32,
        kernel: Kernel,
        ambient: f32,
        self_value: f32,
    ) -> Result<(f32, u32)> {
        if ambient > 0.0 {
            return Ok((ambient, 1));
        }
        let offsets: &[(i32, i32)] = match kernel {
            Kernel::VonNeumann => &N4,
            Kernel::Moore => &N8,
        };

        let mut sum = 0.0f32;
        let mut count = 0u32;
        for (dx, dy) in offsets {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx < 0
                || ny < 0
                || nx >= world.width() as i32
                || ny >= world.height() as i32
            {
                continue; // world edge: neighbor doesn't exist
            }
            sum += world.get(field, nx as u32, ny as u32)?;
            count += 1;
        }
        Ok(if count > 0 {
            (sum / count as f32, count)
        } else {
            (self_value, count)
        })
    }

    /// Apply one diffusion step to `field` inside region `(rx, ry)` of a
    /// world partitioned into `region_size x region_size` regions
    /// (C++ `diffuse`).
    ///
    /// Errors when rate/decay not in `[0,1]` or the region origin lies
    /// outside the world (C++ threw `invalid_argument`/`out_of_range`).
    pub fn diffuse(
        &self,
        world: &mut FieldMap,
        rx: u32,
        ry: u32,
        region_size: u32,
        field: Field,
        rule: &DiffusionRule,
    ) -> Result<()> {
        if rule.rate < 0.0 || rule.rate > 1.0 {
            return Err(Error::invalid_argument(
                "eafar: diffusion rate must be in [0,1]",
            ));
        }
        if rule.decay < 0.0 || rule.decay > 1.0 {
            return Err(Error::invalid_argument(
                "eafar: diffusion decay must be in [0,1]",
            ));
        }

        let x0 = rx.wrapping_mul(region_size);
        let y0 = ry.wrapping_mul(region_size);
        if x0 >= world.width() || y0 >= world.height() {
            return Err(Error::out_of_bounds(
                "eafar: diffusion region out of bounds",
            ));
        }
        let x1 = x0.wrapping_add(region_size).min(world.width());
        let y1 = y0.wrapping_add(region_size).min(world.height());

        // Scratch for the new values: commit only after the whole region
        // is read.
        let region_w = (x1 - x0) as usize;
        let region_h = (y1 - y0) as usize;
        let mut scratch = vec![0.0f32; region_w * region_h];

        for y in y0..y1 {
            for x in x0..x1 {
                let cur = world.get(field, x, y)?;
                let (mean, _count) =
                    self.neighbor_mean(world, field, x, y, rule.kernel, rule.ambient, cur)?;
                let mut next = cur + rule.rate * (mean - cur);
                if rule.decay > 0.0 {
                    next -= rule.decay * next; // decay toward 0
                }
                scratch[(y - y0) as usize * region_w + (x - x0) as usize] = next;
            }
        }

        for y in y0..y1 {
            for x in x0..x1 {
                world.set(
                    field,
                    x,
                    y,
                    scratch[(y - y0) as usize * region_w + (x - x0) as usize],
                )?;
            }
        }
        Ok(())
    }

    /// Bulk: run the rule on every ACTIVE region (ascending id), reusing
    /// the same scratch buffer per region. Deterministic (C++
    /// `diffuse_active`).
    pub fn diffuse_active(
        &self,
        world: &mut FieldMap,
        active_region_ids: &BTreeSet<u32>,
        regions_x: u32,
        region_size: u32,
        field: Field,
        rule: &DiffusionRule,
    ) -> Result<()> {
        for &id in active_region_ids {
            // ascending (BTreeSet)
            let rx = id % regions_x;
            let ry = id / regions_x;
            self.diffuse(world, rx, ry, region_size, field, rule)?;
        }
        Ok(())
    }
}

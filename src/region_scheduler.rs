//! S3: Region Scheduler.
//!
//! C++ lineage: `include/eafar/region_scheduler.hpp` +
//! `src/region_scheduler.cpp`.
//!
//! THE core thesis of EAFAR: an event must not wake the whole world, only
//! its region. The world is partitioned into fixed-size regions; each
//! region is either SLEEPING (zero work) or ACTIVE (local computation runs).
//!
//! Lifecycle per region: `Event -> wake -> local computation (tick) ->
//! idle for N ticks -> sleep`.
//!
//! Determinism: active regions are iterated in ascending region-id order
//! (ordered containers only — never hash order).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::field_map::FieldMap;

/// Per-tick local computation for one ACTIVE region (C++ `RegionWork`).
///
/// Rust porting note: the C++ `std::function` captured the scheduler by
/// reference when the work needed to wake other regions. Re-entrant
/// borrows are not expressible in Rust, so the scheduler is passed to the
/// callback explicitly as its fourth argument.
pub type RegionWork =
    Arc<dyn Fn(u32, u32, &mut FieldMap, &mut RegionScheduler) -> Result<()>>;

/// Partitions the world into `region_size x region_size` regions and tracks
/// which of them are active.
///
/// C++ lineage: `class RegionScheduler`.
pub struct RegionScheduler {
    region_size: u32,
    regions_x: u32,
    regions_y: u32,
    idle_before_sleep: u32,

    /// Active region ids, ascending (C++ `std::set<std::uint32_t>`).
    active: BTreeSet<u32>,
    /// Region id -> ticks since last wake (C++ `std::map`).
    idle_ticks: BTreeMap<u32, u32>,
    work: Option<RegionWork>,

    /// Regions woken during the current tick's work phase. Used to grant a
    /// fresh lifetime (skip aging) to regions re-woken by their own work.
    /// Temporary scratch only — cleared at the start of every [`tick`]
    /// (C++ `woken_during_work_`).
    woken_during_work: BTreeSet<u32>,
}

impl RegionScheduler {
    /// Build a scheduler for a `world_w x world_h` world partitioned into
    /// `region_size x region_size` regions.
    ///
    /// A world not divisible by `region_size` gets partial edge regions
    /// (ceil division). Errors when `region_size == 0` (C++ threw
    /// `std::invalid_argument`).
    pub fn new(world_w: u32, world_h: u32, region_size: u32) -> Result<Self> {
        if region_size == 0 {
            return Err(Error::invalid_argument("eafar: region_size must be > 0"));
        }
        Ok(RegionScheduler {
            region_size,
            // Ceil division.
            regions_x: (world_w + region_size - 1) / region_size,
            regions_y: (world_h + region_size - 1) / region_size,
            idle_before_sleep: 1,
            active: BTreeSet::new(),
            idle_ticks: BTreeMap::new(),
            work: None,
            woken_during_work: BTreeSet::new(),
        })
    }

    // --- geometry ---

    /// Region edge length in cells.
    pub fn region_size(&self) -> u32 {
        self.region_size
    }

    /// Regions along x.
    pub fn regions_x(&self) -> u32 {
        self.regions_x
    }

    /// Regions along y.
    pub fn regions_y(&self) -> u32 {
        self.regions_y
    }

    /// Total number of regions.
    pub fn region_count(&self) -> u32 {
        self.regions_x * self.regions_y
    }

    /// Region coordinate along x for a world coordinate.
    pub fn region_x(&self, wx: u32) -> u32 {
        wx / self.region_size
    }

    /// Region coordinate along y for a world coordinate.
    pub fn region_y(&self, wy: u32) -> u32 {
        wy / self.region_size
    }

    /// World coord -> region id (row-major).
    pub fn region_id(&self, wx: u32, wy: u32) -> u32 {
        self.region_y(wy) * self.regions_x + self.region_x(wx)
    }

    // --- state ---

    /// True if region `(rx, ry)` is active (out-of-range → false).
    pub fn is_active(&self, rx: u32, ry: u32) -> bool {
        if rx >= self.regions_x || ry >= self.regions_y {
            return false;
        }
        self.is_active_id(ry * self.regions_x + rx)
    }

    /// True if the region with id `id` is active.
    pub fn is_active_id(&self, id: u32) -> bool {
        self.active.contains(&id)
    }

    /// Number of active regions.
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Deterministic snapshot of active region ids, ascending
    /// (C++ `active_ids`).
    pub fn active_ids(&self) -> &BTreeSet<u32> {
        &self.active
    }

    // --- activation ---

    /// Wake the region containing world cell `(wx, wy)`
    /// (C++ `wake`).
    pub fn wake(&mut self, wx: u32, wy: u32) -> Result<()> {
        let (rx, ry) = (self.region_x(wx), self.region_y(wy));
        self.wake_region(rx, ry)
    }

    /// Wake region `(rx, ry)`. Errors when out of bounds (C++ threw
    /// `std::out_of_range`). Wake resets the idle counter.
    pub fn wake_region(&mut self, rx: u32, ry: u32) -> Result<()> {
        if rx >= self.regions_x || ry >= self.regions_y {
            return Err(Error::out_of_bounds("eafar: wake_region out of bounds"));
        }
        let id = ry * self.regions_x + rx;
        self.active.insert(id);
        self.idle_ticks.insert(id, 0); // wake resets the idle counter
        // Matters only if called during tick's work phase.
        self.woken_during_work.insert(id);
        Ok(())
    }

    /// Put region `(rx, ry)` to sleep. Errors when out of bounds.
    pub fn sleep_region(&mut self, rx: u32, ry: u32) -> Result<()> {
        if rx >= self.regions_x || ry >= self.regions_y {
            return Err(Error::out_of_bounds("eafar: sleep_region out of bounds"));
        }
        let id = ry * self.regions_x + rx;
        self.active.remove(&id);
        self.idle_ticks.remove(&id);
        Ok(())
    }

    // --- work ---

    /// Install the per-region work callback (C++ `set_region_work`).
    pub fn set_region_work(
        &mut self,
        work: impl Fn(u32, u32, &mut FieldMap, &mut RegionScheduler) -> Result<()> + 'static,
    ) {
        self.work = Some(Arc::new(work));
    }

    // --- ticking ---

    /// Idle ticks a region may spend before it sleeps (C++
    /// `idle_ticks_before_sleep`).
    pub fn idle_ticks_before_sleep(&self) -> u32 {
        self.idle_before_sleep
    }

    /// Configure the idle-ticks-before-sleep threshold.
    pub fn set_idle_ticks_before_sleep(&mut self, n: u32) {
        self.idle_before_sleep = n;
    }

    /// Advance one tick: run work on every active region (ascending id
    /// order), then age idle regions; regions idle >=
    /// `idle_ticks_before_sleep` sleep (C++ `tick`).
    pub fn tick(&mut self, world: &mut FieldMap) -> Result<()> {
        // Iterate active regions in ascending id order (deterministic).
        // Work may wake new regions or re-wake the current one, so
        // snapshot first.
        let snapshot: Vec<u32> = self.active.iter().copied().collect();
        self.woken_during_work.clear();

        for id in snapshot {
            if let Some(work) = self.work.clone() {
                let rx = id % self.regions_x;
                let ry = id / self.regions_x;
                work(rx, ry, world, self)?;
            }

            // A region re-woken during work gets a fresh lifetime (idle
            // reset); it is not aged this tick.
            if self.woken_during_work.contains(&id) {
                continue;
            }

            match self.idle_ticks.get_mut(&id) {
                None => continue, // work slept the region; nothing to age
                Some(counter) => *counter += 1,
            }
            let aged = self.idle_ticks[&id];
            if aged >= self.idle_before_sleep {
                self.active.remove(&id);
                self.idle_ticks.remove(&id);
            }
        }
        Ok(())
    }
}

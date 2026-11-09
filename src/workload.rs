//! Reference cellular-automaton workloads: dense synchronous vs sparse
//! event-driven execution of the SAME rule on the SAME world.
//!
//! This module exists so that the benchmark (`src/bin/eafar-bench.rs`), the
//! determinism property tests and the `viz` crate all run identical rule
//! kernels through the same code path — only the set of VISITED cells
//! differs between the two execution models:
//!
//! * **Dense synchronous** (`DenseSession`): every generation recomputes
//!   every cell of the world from the previous state (textbook double
//!   buffer). Cost is `Θ(world cells)` per generation, forever.
//! * **Sparse event-driven** (`SparseSession`): the world is partitioned
//!   into regions ([`RegionScheduler`], S3); only ACTIVE regions recompute.
//!   A region stays awake exactly while its cells (or a neighbour's border
//!   cells) change; silent regions sleep after one idle tick and cost zero.
//!   Cost tracks the ACTIVE SET, which decays as the world settles.
//!
//! Both drivers use the same per-cell kernel ([`rule_next`]) reading the
//! same old-state array, so — by the wake-policy argument in
//! `docs/model.md` — they produce **bit-identical trajectories**. The
//! property tests and the benchmark cross-check assert exactly that.
//!
//! Determinism: fixed iteration orders (row-major cells, ascending region
//! ids), fixed `f32` operation order, no wall clock, no hash-order
//! containers. State hashes (FNV-1a over the `f32` bit patterns) are
//! stable across runs.

use std::cell::RefCell;
use std::rc::Rc;

use crate::error::Result;
use crate::field::Field;
use crate::field_map::FieldMap;
use crate::region_scheduler::RegionScheduler;
use crate::rng::SplitMix64;
use crate::snapshot::WorldSnapshot;

// ---------------------------------------------------------------------------
// Workload selection
// ---------------------------------------------------------------------------

/// One of the three reference rules (C++ lineage: none — added for the
/// 2026 audit as shared bench/visualizer/test workloads).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Workload {
    /// Conway's Game of Life on `Field::Population` (1.0 = alive).
    /// Moore neighborhood, classic B3/S23 rule. Random soup settles into
    /// still lifes and oscillators: activity decays but never reaches zero.
    Conway,
    /// Heat diffusion on `Field::Temperature`: von Neumann neighbor mean
    /// relaxation (rate [`HEAT_RATE`]) with multiplicative decay
    /// ([`HEAT_DECAY_FACTOR`]). Gaussian-free polynomial hotspots; the
    /// field decays to exact `0.0`, after which every region sleeps.
    Heat,
    /// Threshold contagion (SIR) on `Field::Population`: susceptible 0.0,
    /// infected `1.0 + 0.25 * k` (stages are exact binary fractions),
    /// recovered/immune 2.0. Infection requires >=
    /// [`CONTAGION_THRESHOLD_COUNT`] of 8 Moore neighbors infected. A
    /// wavefront sweeps the world, then everything sleeps.
    Contagion,
}

impl Workload {
    /// All workloads in declaration order.
    pub const ALL: [Workload; 3] = [Workload::Conway, Workload::Heat, Workload::Contagion];

    /// Stable machine-readable name (`conway` / `heat` / `contagion`).
    pub const fn name(self) -> &'static str {
        match self {
            Workload::Conway => "conway",
            Workload::Heat => "heat",
            Workload::Contagion => "contagion",
        }
    }

    /// Human-readable description of the rule.
    pub const fn description(self) -> &'static str {
        match self {
            Workload::Conway => "Conway Life B3/S23 (Population)",
            Workload::Heat => "heat diffusion + decay (Temperature)",
            Workload::Contagion => "threshold contagion SIR (Population)",
        }
    }

    /// The SoA field the rule reads and writes.
    pub const fn field(self) -> Field {
        match self {
            Workload::Conway | Workload::Contagion => Field::Population,
            Workload::Heat => Field::Temperature,
        }
    }

    /// Parse from a workload name ([`Workload::name`]). Errors on unknown
    /// names — used by CLI flags and the visualizer selector.
    pub fn from_name(name: &str) -> Result<Self> {
        for w in Self::ALL {
            if w.name() == name {
                return Ok(w);
            }
        }
        Err(crate::error::Error::not_found(&format!(
            "eafar: unknown workload '{name}' (known: conway, heat, contagion)"
        )))
    }
}

// Rule constants. All are exact or pre-computed constants so the `f32`
// operation order is fixed by construction (see `docs/model.md`).
/// Heat: fraction of the neighbor mean mixed in per step.
const HEAT_RATE: f32 = 0.2;
/// Heat: multiplicative decay factor applied after the relaxation. A power
/// of two on purpose: multiplying by 0.5 is an exact exponent decrement,
/// so the decay is strictly decreasing and reaches exact `0.0` (no
/// denormal fixed point, no rounding noise).
const HEAT_DECAY_FACTOR: f32 = 0.5;
/// Contagion: infected neighbors (of 8) required to infect a susceptible.
const CONTAGION_THRESHOLD_COUNT: f32 = 2.0;
/// Contagion: infection duration in generations (`0.25`-stages).
const CONTAGION_STAGES: f32 = 4.0;
/// Conway initial alive density of the random soup.
const CONWAY_SEED_DENSITY: f32 = 0.35;

/// Moore neighborhood in fixed order (same offsets as the diffusion
/// engine's `N8`): E, W, S, N, then the diagonals.
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
/// Von Neumann neighborhood in fixed order (matches the diffusion engine).
const N4: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

// ---------------------------------------------------------------------------
// The shared per-cell kernel
// ---------------------------------------------------------------------------

/// Compute the next value of one cell from the OLD state slice.
///
/// `cur` is the full row-major field array of a `w x h` world. Returns
/// `(next_value, changed)` where `changed` is a bit-exact comparison
/// (`f32::to_bits`) against the current value. This is THE kernel used by
/// both execution models — the only difference between dense and sparse
/// runs is which cells this function is invoked for.
#[inline]
fn rule_next(rule: Workload, cur: &[f32], w: u32, h: u32, x: u32, y: u32) -> (f32, bool) {
    let idx = y as usize * w as usize + x as usize;
    let cur_value = cur[idx];

    let next = match rule {
        Workload::Conway => {
            // Sum of 8 Moore neighbors; outside the world counts as dead.
            let mut n = 0.0f32;
            for &(dx, dy) in N8.iter() {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                n += cur[ny as usize * w as usize + nx as usize];
            }
            let alive = cur_value >= 0.5;
            if alive {
                if n == 2.0 || n == 3.0 {
                    1.0
                } else {
                    0.0
                }
            } else if n == 3.0 {
                1.0
            } else {
                0.0
            }
        }
        Workload::Heat => {
            // Relax toward the von Neumann neighbor mean, then decay.
            let mut sum = 0.0f32;
            let mut count = 0u32;
            for &(dx, dy) in N4.iter() {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                sum += cur[ny as usize * w as usize + nx as usize];
                count += 1;
            }
            let mean = if count > 0 {
                sum / count as f32
            } else {
                cur_value
            };
            // Fixed op order: difference, mix, decay.
            let relaxed = cur_value + (mean - cur_value) * HEAT_RATE;
            relaxed * HEAT_DECAY_FACTOR
        }
        Workload::Contagion => {
            // S = 0.0, I stages = 1.0 + 0.25*k, R = 2.0 (all exact f32).
            if cur_value >= 2.0 {
                2.0 // recovered: immune forever
            } else if cur_value >= 1.0 {
                // advance the infection stage (1.75 + 0.25 == 2.0 exactly)
                let next_stage = cur_value + 0.25;
                if next_stage >= 1.0 + 0.25 * CONTAGION_STAGES {
                    2.0
                } else {
                    next_stage
                }
            } else {
                // susceptible: count infected Moore neighbors
                let mut infected = 0.0f32;
                for &(dx, dy) in N8.iter() {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let v = cur[ny as usize * w as usize + nx as usize];
                    if v >= 1.0 && v < 2.0 {
                        infected += 1.0;
                    }
                }
                if infected >= CONTAGION_THRESHOLD_COUNT {
                    1.0
                } else {
                    0.0
                }
            }
        }
    };

    let changed = next.to_bits() != cur_value.to_bits();
    (next, changed)
}

// ---------------------------------------------------------------------------
// Deterministic seeding (identical for both execution models)
// ---------------------------------------------------------------------------

/// Seed `world` for `rule` from `rng` (SplitMix64 stream). Row-major, fixed
/// order — the same `(rule, seed)` pair produces the bit-same initial world
/// for the dense and the sparse driver.
pub fn seed_world(rule: Workload, world: &mut FieldMap, rng: &mut SplitMix64) {
    let (w, h) = (world.width(), world.height());
    match rule {
        Workload::Conway => {
            for y in 0..h {
                for x in 0..w {
                    let alive = rng.next_unit() < CONWAY_SEED_DENSITY;
                    let _ = world.set(rule.field(), x, y, if alive { 1.0 } else { 0.0 });
                }
            }
        }
        Workload::Heat => {
            // Polynomial hotspots (no libm): v = amp * max(0, 1 - d/radius)^2.
            // Amplitudes are jittered from the RNG stream so that the seed
            // matters (same seed -> same world, different seed -> different
            // world), keeping the draw order fixed.
            let jitter: Vec<f32> = (0..5).map(|_| 0.8 + 0.4 * rng.next_unit()).collect();
            let spots: [(f32, f32, f32, f32); 5] = [
                (0.25 * w as f32, 0.30 * h as f32, 1.0 * jitter[0], 14.0),
                (0.70 * w as f32, 0.25 * h as f32, 0.9 * jitter[1], 12.0),
                (0.50 * w as f32, 0.60 * h as f32, 0.8 * jitter[2], 16.0),
                (0.20 * w as f32, 0.75 * h as f32, 0.7 * jitter[3], 10.0),
                (0.80 * w as f32, 0.70 * h as f32, 0.6 * jitter[4], 12.0),
            ];
            for y in 0..h {
                for x in 0..w {
                    let mut v = 0.0f32;
                    for &(cx, cy, amp, radius) in spots.iter() {
                        let dx = x as f32 - cx;
                        let dy = y as f32 - cy;
                        let d = (dx * dx + dy * dy).sqrt();
                        let t = 1.0 - d / radius;
                        if t > 0.0 {
                            v += amp * t * t;
                        }
                    }
                    if v > 0.0 {
                        let _ = world.set(rule.field(), x, y, v);
                    }
                }
            }
        }
        Workload::Contagion => {
            // All susceptible, one central infected disk + scattered seeds.
            let (cx, cy) = (w / 2, h / 2);
            for y in 0..h {
                for x in 0..w {
                    let dx = x as i32 - cx as i32;
                    let dy = y as i32 - cy as i32;
                    if dx * dx + dy * dy <= 36 {
                        let _ = world.set(rule.field(), x, y, 1.0);
                    }
                }
            }
            for _ in 0..40 {
                let x = rng.next_u32(w);
                let y = rng.next_u32(h);
                let _ = world.set(rule.field(), x, y, 1.0);
            }
        }
    }
}

/// Wake every region that contains at least one non-zero cell of `field`.
/// Used after seeding and after restoring a rewind frame.
fn wake_nonzero_regions(world: &FieldMap, sched: &mut RegionScheduler, field: Field) {
    let (w, h) = (world.width(), world.height());
    for y in 0..h {
        for x in 0..w {
            if world.get(field, x, y).unwrap_or(0.0).to_bits() != 0 {
                let _ = sched.wake(x, y);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// State hash (FNV-1a over f32 bit patterns)
// ---------------------------------------------------------------------------

/// FNV-1a 64-bit hash over the bit patterns of one field, row-major.
///
/// Hashing BITS (not float equality) makes NaN payloads and `-0.0` count —
/// the same standard as [`FieldMap::bitwise_equal`]. Same state → same
/// hash, every run, every platform.
pub fn hash_field(world: &FieldMap, field: Field) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    for &v in world.data(field) {
        hash ^= v.to_bits() as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

// ---------------------------------------------------------------------------
// Per-generation statistics
// ---------------------------------------------------------------------------

/// Statistics of one generation, identical shape for both execution models.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GenerationStats {
    /// Generation counter after the step (starts at 1).
    pub generation: u64,
    /// Regions that ran their local computation this generation.
    pub active_regions: usize,
    /// Cells recomputed this generation (active regions x region cells,
    /// edge regions counted exactly).
    pub active_cells: usize,
    /// Events executed: one cell recomputation = one event
    /// (== `active_cells` for the sparse driver, `world cells` for dense).
    pub cell_updates: usize,
    /// Cells whose value actually changed (bit-exact) this generation.
    pub changed_cells: usize,
}

impl GenerationStats {
    /// Active-cell ratio of this generation: `active_cells / world cells`.
    pub fn active_ratio(&self, world_cells: usize) -> f32 {
        if world_cells == 0 {
            return 0.0;
        }
        self.active_cells as f32 / world_cells as f32
    }
}

// ---------------------------------------------------------------------------
// Sparse event-driven driver
// ---------------------------------------------------------------------------

/// Shared scratch between the session and the installed `RegionWork`
/// closure: pending writes (cell index, new value) for the generation in
/// flight. `Rc<RefCell>` is enough — the scheduler is single-threaded by
/// design (determinism), and `RegionWork` deliberately carries no
/// `Send + Sync` bound.
#[derive(Default)]
struct SparseScratch {
    /// Writes accumulated this generation, in execution order (ascending
    /// region id, row-major inside a region). Each cell is written at most
    /// once per generation — the order carries no algebraic weight.
    writes: Vec<(u32, f32)>,
}

impl SparseScratch {
    fn clear(&mut self) {
        self.writes.clear();
    }
}

/// A checkpoint of the whole sparse session: world state + active set +
/// counters. Restoring it yields TRUE rewind: continuing from the
/// checkpoint reproduces the uninterrupted trajectory bit-exactly (the
/// property tests assert this). `WorldSnapshot` alone would restore the
/// fields but not "which regions are awake" — that state lives here too.
#[derive(Debug, Clone)]
pub struct SessionCheckpoint {
    world: WorldSnapshot,
    active_ids: Vec<u32>,
    generation: u64,
    events_total: u64,
}

impl SessionCheckpoint {
    /// Build a checkpoint from explicit parts — used by the visualizer's
    /// rewind frames, which capture exactly these pieces of a session.
    pub fn from_parts(
        world: &FieldMap,
        active_ids: &[u32],
        generation: u64,
        events_total: u64,
    ) -> Self {
        SessionCheckpoint {
            world: WorldSnapshot::capture(world),
            active_ids: active_ids.to_vec(),
            generation,
            events_total,
        }
    }
}

/// Sparse event-driven execution of a [`Workload`]: state lives in a
/// [`FieldMap`], activity in a [`RegionScheduler`]; every generation runs
/// `sched.tick` over the active set, buffers the writes, and commits them
/// after the whole active set has been processed (read-old/write-new, so
/// the semantics are the SAME synchronous update dense executes — only
/// fewer cells are touched).
pub struct SparseSession {
    world: FieldMap,
    sched: RegionScheduler,
    rule: Workload,
    scratch: Rc<RefCell<SparseScratch>>,
    generation: u64,
    events_total: u64,
}

impl SparseSession {
    /// Build a session for a `world_w x world_h` world partitioned into
    /// `region_size x region_size` regions, seeded deterministically from
    /// `seed`. Regions containing non-zero cells start awake.
    pub fn new(
        rule: Workload,
        world_w: u32,
        world_h: u32,
        region_size: u32,
        seed: u64,
    ) -> Result<Self> {
        let mut world = FieldMap::new(world_w, world_h);
        let mut sched = RegionScheduler::new(world_w, world_h, region_size)?;
        seed_world(rule, &mut world, &mut SplitMix64::new(seed));
        wake_nonzero_regions(&world, &mut sched, rule.field());

        let scratch = Rc::new(RefCell::new(SparseScratch::default()));
        let closure_scratch = scratch.clone();
        sched.set_region_work(move |rx, ry, world, sched| {
            step_region_work(rule, rx, ry, world, sched, &mut closure_scratch.borrow_mut());
            Ok(())
        });

        Ok(SparseSession {
            world,
            sched,
            rule,
            scratch,
            generation: 0,
            events_total: 0,
        })
    }

    /// The rule being simulated.
    pub fn rule(&self) -> Workload {
        self.rule
    }

    /// Read access to the world state.
    pub fn world(&self) -> &FieldMap {
        &self.world
    }

    /// Mutable access to the world state — the injection point for external
    /// events (poke cells, apply impulses). The caller drives the wake
    /// contract itself: wake the touched region (or rely on the next
    /// generation noticing changes inside an already-active region) before
    /// stepping. This is exactly how the event-driven model is meant to be
    /// used: events mutate state, the scheduler reacts.
    pub fn world_mut(&mut self) -> &mut FieldMap {
        &mut self.world
    }

    /// Read access to the scheduler (active set).
    pub fn sched(&self) -> &RegionScheduler {
        &self.sched
    }

    /// Mutable access to the scheduler — wake/sleep regions from outside
    /// (event handlers, user clicks in the visualizer).
    pub fn sched_mut(&mut self) -> &mut RegionScheduler {
        &mut self.sched
    }

    /// Generations completed so far.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Cumulative events (cell recomputations) since creation.
    pub fn events_total(&self) -> u64 {
        self.events_total
    }

    /// Current capacity of the pending-write buffer, in slots (one slot is
    /// an index + a value). Exposed for memory accounting in benchmarks;
    /// the buffer only grows to hold the ACTIVE cells of one generation.
    pub fn scratch_capacity(&self) -> usize {
        self.scratch.borrow().writes.capacity()
    }

    /// Exact number of cells inside region `id` (edge regions may be
    /// partial).
    fn region_cell_count(&self, id: u32) -> usize {
        let rs = self.sched.region_size() as usize;
        let rx = (id % self.sched.regions_x()) as usize * rs;
        let ry = (id / self.sched.regions_x()) as usize * rs;
        let x1 = (rx + rs).min(self.world.width() as usize);
        let y1 = (ry + rs).min(self.world.height() as usize);
        (x1 - rx) * (y1 - ry)
    }

    /// Advance one generation: tick every active region (ascending id),
    /// then commit the buffered writes to the world. Returns the
    /// generation statistics.
    pub fn step_generation(&mut self) -> Result<GenerationStats> {
        self.scratch.borrow_mut().clear();

        // The tick visits exactly the regions active at tick start.
        let active_ids: Vec<u32> = self.sched.active_ids().iter().copied().collect();
        self.sched.tick(&mut self.world)?;

        // Commit: apply the buffered writes in generation order.
        let field = self.rule.field();
        {
            let slice = self.world.data_mut(field);
            let scratch = self.scratch.borrow();
            for &(idx, value) in scratch.writes.iter() {
                slice[idx as usize] = value;
            }
        }

        let active_regions = active_ids.len();
        let active_cells: usize = active_ids.iter().copied().map(|id| self.region_cell_count(id)).sum();
        let changed_cells = self.scratch.borrow().writes.len();

        self.generation += 1;
        self.events_total += active_cells as u64;

        Ok(GenerationStats {
            generation: self.generation,
            active_regions,
            active_cells,
            cell_updates: active_cells,
            changed_cells,
        })
    }

    /// FNV-1a hash of the rule field's bit patterns.
    pub fn state_hash(&self) -> u64 {
        hash_field(&self.world, self.rule.field())
    }

    /// Bit-exact checkpoint of world + active set + counters.
    pub fn checkpoint(&self) -> SessionCheckpoint {
        SessionCheckpoint {
            world: WorldSnapshot::capture(&self.world),
            active_ids: self.sched.active_ids().iter().copied().collect(),
            generation: self.generation,
            events_total: self.events_total,
        }
    }

    /// Restore a checkpoint (true rewind). The active set is rebuilt so the
    /// continuation from the checkpoint is bit-identical to the original
    /// trajectory.
    pub fn restore(&mut self, cp: &SessionCheckpoint) -> Result<()> {
        cp.world.restore(&mut self.world)?;
        // Rebuild the active set exactly.
        let current: Vec<u32> = self.sched.active_ids().iter().copied().collect();
        let rx_count = self.sched.regions_x();
        for id in current {
            let _ = self.sched.sleep_region(id % rx_count, id / rx_count);
        }
        for &id in &cp.active_ids {
            let _ = self.sched.wake_region(id % rx_count, id / rx_count);
        }
        self.scratch.borrow_mut().clear();
        self.generation = cp.generation;
        self.events_total = cp.events_total;
        Ok(())
    }
}

/// The per-region work installed into the scheduler: recompute the rule for
/// every cell of region `(rx, ry)` reading the OLD state, buffer changed
/// cells as writes, and keep/wake the regions whose inputs changed:
///
/// * any change in this region → re-wake it (it must recompute next
///   generation);
/// * a change in a border cell → wake every region containing a cell
///   within Moore distance 1 of the change (their inputs changed);
/// * no change → the region is not re-woken and the scheduler's idle
///   aging (`idle_ticks_before_sleep = 1`) puts it to sleep.
fn step_region_work(
    rule: Workload,
    rx: u32,
    ry: u32,
    world: &mut FieldMap,
    sched: &mut RegionScheduler,
    scratch: &mut SparseScratch,
) {
    let writes_before = scratch.writes.len();
    let rs = sched.region_size();
    let (w, h) = (world.width(), world.height());
    let field = rule.field();
    let cur = world.data(field);
    let x0 = rx * rs;
    let y0 = ry * rs;
    let x1 = (x0 + rs).min(w);
    let y1 = (y0 + rs).min(h);

    for y in y0..y1 {
        for x in x0..x1 {
            let (next, changed) = rule_next(rule, cur, w, h, x, y);
            if changed {
                let idx = y as usize * w as usize + x as usize;
                scratch.writes.push((idx as u32, next));

                // Border cells influence neighbor regions (Moore ring).
                // Interior changes only influence this region, which is
                // re-woken below anyway.
                let on_border = x == x0 || x == x1 - 1 || y == y0 || y == y1 - 1;
                if on_border {
                    for &(dx, dy) in N8.iter() {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        let nrx = nx as u32 / rs;
                        let nry = ny as u32 / rs;
                        if nrx != rx || nry != ry {
                            let _ = sched.wake_region(nrx, nry);
                        }
                    }
                }
            }
        }
    }

    // Keep this region awake iff it produced at least one write: waking
    // during the work phase marks it `woken_during_work`, so the scheduler
    // skips aging and it stays active for the next generation. A silent
    // region is not re-woken and sleeps via idle aging
    // (`idle_ticks_before_sleep == 1`).
    if scratch.writes.len() > writes_before {
        let _ = sched.wake_region(rx, ry);
    }
}

// ---------------------------------------------------------------------------
// Dense synchronous driver
// ---------------------------------------------------------------------------

/// Dense synchronous execution: every generation recomputes every cell from
/// the previous state (textbook double buffer) and copies the result back.
/// This is the REFERENCE model — full cost every generation, regardless of
/// how much of the world actually changed.
pub struct DenseSession {
    world: FieldMap,
    next: Vec<f32>,
    rule: Workload,
    generation: u64,
    events_total: u64,
}

impl DenseSession {
    /// Build a dense session seeded exactly like [`SparseSession::new`].
    pub fn new(rule: Workload, world_w: u32, world_h: u32, seed: u64) -> Self {
        let mut world = FieldMap::new(world_w, world_h);
        seed_world(rule, &mut world, &mut SplitMix64::new(seed));
        let n = world.cell_count();
        DenseSession {
            world,
            next: vec![0.0; n],
            rule,
            generation: 0,
            events_total: 0,
        }
    }

    /// The rule being simulated.
    pub fn rule(&self) -> Workload {
        self.rule
    }

    /// Read access to the world state.
    pub fn world(&self) -> &FieldMap {
        &self.world
    }

    /// Generations completed so far.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Cumulative events (cell recomputations): dense recomputes the whole
    /// world every generation.
    pub fn events_total(&self) -> u64 {
        self.events_total
    }

    /// Advance one generation over the whole world. Returns the number of
    /// cells whose value changed.
    pub fn step_generation(&mut self) -> u64 {
        let field = self.rule.field();
        let (w, h) = (self.world.width(), self.world.height());
        let cur = self.world.data(field);
        let mut changed = 0u64;
        for y in 0..h {
            for x in 0..w {
                let idx = y as usize * w as usize + x as usize;
                let (next, ch) = rule_next(self.rule, cur, w, h, x, y);
                self.next[idx] = next;
                if ch {
                    changed += 1;
                }
            }
        }
        self.world.data_mut(field).copy_from_slice(&self.next);
        self.generation += 1;
        self.events_total += self.world.cell_count() as u64;
        changed
    }

    /// FNV-1a hash of the rule field's bit patterns.
    pub fn state_hash(&self) -> u64 {
        hash_field(&self.world, self.rule.field())
    }
}

// ---------------------------------------------------------------------------
// Tests: kernel and seeding determinism
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workload_names_round_trip() {
        for w in Workload::ALL {
            assert_eq!(Workload::from_name(w.name()).unwrap(), w);
        }
        assert!(Workload::from_name("nope").is_err());
    }

    #[test]
    fn conway_blinker_oscillates() {
        // A vertical blinker in a 5x5 world must oscillate with period 2.
        let mut world = FieldMap::new(5, 5);
        for y in 1..4 {
            world.set(Field::Population, 2, y, 1.0).unwrap();
        }
        let mut s = DenseSession {
            world,
            next: vec![0.0; 25],
            rule: Workload::Conway,
            generation: 0,
            events_total: 0,
        };
        s.step_generation(); // -> horizontal
        let h_sum: f32 = s.world().data(Field::Population).iter().sum();
        assert!((h_sum - 3.0).abs() < 1e-6, "expected 3 alive, got {h_sum}");
        let horizontal = s.world().get(Field::Population, 1, 2).unwrap() == 1.0;
        assert!(horizontal, "blinker should be horizontal now");
        s.step_generation(); // -> vertical again
        assert_eq!(s.state_hash(), {
            let mut w0 = FieldMap::new(5, 5);
            for y in 1..4 {
                w0.set(Field::Population, 2, y, 1.0).unwrap();
            }
            hash_field(&w0, Field::Population)
        });
    }

    #[test]
    fn heat_decays_to_exact_zero() {
        let mut s = DenseSession::new(Workload::Heat, 32, 32, 1);
        for _ in 0..600 {
            s.step_generation();
        }
        let all_zero = s.world().data(Field::Temperature).iter().all(|v| v.to_bits() == 0);
        assert!(all_zero, "heat must decay to exact 0.0 within 600 generations");
    }

    #[test]
    fn contagion_stages_are_exact() {
        // An isolated infected cell advances 1.0 -> 1.25 -> ... -> 2.0.
        let mut s = DenseSession::new(Workload::Contagion, 8, 8, 0);
        s.world.set(Field::Population, 4, 4, 1.0).unwrap();
        let mut stages = vec![s.world.get(Field::Population, 4, 4).unwrap()];
        for _ in 0..4 {
            s.step_generation();
            stages.push(s.world.get(Field::Population, 4, 4).unwrap());
        }
        assert_eq!(stages, vec![1.0, 1.25, 1.5, 1.75, 2.0]);
        // And a recovered cell never changes again.
        for _ in 0..10 {
            s.step_generation();
        }
        assert_eq!(s.world.get(Field::Population, 4, 4).unwrap(), 2.0);
    }

    #[test]
    fn hash_field_is_fnv1a_and_bit_exact() {
        // Hand-computed FNV-1a over the bit patterns of [0.0f32, 1.0f32].
        let mut world = FieldMap::new(2, 1);
        world.set(Field::Population, 0, 0, 0.0).unwrap();
        world.set(Field::Population, 1, 0, 1.0).unwrap();
        // 0.0f32 bits = 0x00000000, 1.0f32 bits = 0x3f800000.
        let mut h = 0xcbf29ce484222325u64;
        for bits in [0x0000_0000u32, 0x3f80_0000u32] {
            h ^= bits as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        assert_eq!(hash_field(&world, Field::Population), h);
        // -0.0 has different bits than 0.0 and must hash differently.
        world.set(Field::Population, 0, 0, -0.0).unwrap();
        assert_ne!(hash_field(&world, Field::Population), {
            world.set(Field::Population, 0, 0, 0.0).unwrap();
            hash_field(&world, Field::Population)
        });
    }

    #[test]
    fn seeding_is_identical_for_same_seed() {
        let mut a = FieldMap::new(64, 64);
        let mut b = FieldMap::new(64, 64);
        seed_world(Workload::Conway, &mut a, &mut SplitMix64::new(123));
        seed_world(Workload::Conway, &mut b, &mut SplitMix64::new(123));
        assert!(a.bitwise_equal(&b));
        let mut c = FieldMap::new(64, 64);
        seed_world(Workload::Conway, &mut c, &mut SplitMix64::new(124));
        assert!(!a.bitwise_equal(&c));
    }
}

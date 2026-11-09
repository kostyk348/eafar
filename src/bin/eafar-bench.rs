//! EAFAR benchmark — Rust port of `benchmarks/bench_main.cpp` plus the
//! 2026 audit comparison suite.
//!
//! Part 1 (audit 2026): dense synchronous vs sparse event-driven execution
//! of the same three rule kernels (Conway life, heat diffusion, threshold
//! contagion) on the same world, printing the metric table that
//! `docs/benchmarks.md` is generated from. The per-cell kernel is shared
//! (`eafar::workload`), so the ONLY difference between the two modes is
//! which cells get visited.
//!
//! Part 2 (legacy, C++ lineage): the core-thesis microbenchmark — in a
//! sparse world, per-tick cost must be proportional to the ACTIVE area,
//! not the world area. Verified two ways:
//!
//!   A) Fixed world 4096x4096 (16384 regions), 16 active regions (0.1%):
//!      region-driven tick must be ~1000x cheaper than full update.
//!   B) Scaling: same 16 active regions, world 2048x2048 vs 4096x4096.
//!      Region tick cost must stay ~constant (sublinear in world size),
//!      while full update doubles.
//!
//! The per-cell operation is identical in all cases (temperature decay
//! toward ambient) — the only difference is WHICH cells are visited.
//!
//! Run: `cargo run --release --bin eafar-bench`

use std::time::Instant;

use eafar::prelude::*;

/// Region edge used by the legacy microbenchmark (matches the C++ `RS`).
const RS: u32 = 32;
/// Ticks per measurement (matches the C++ `TICKS`).
const TICKS: usize = 20;

// ---------------------------------------------------------------------------
// Part 1: dense synchronous vs sparse event-driven
// ---------------------------------------------------------------------------

/// World edge for the comparison runs (1024x1024 = 1,048,576 cells =
/// 32x32 = 1024 regions of 32x32).
const WORLD: u32 = 1024;
/// Generations per run.
const GENS: usize = 300;
/// Seed for all comparison runs.
const SEED: u64 = 20260731;

struct RunOutcome {
    wall_secs: f64,
    events: u64,
    avg_active_cells: f64,
    avg_active_ratio: f64,
    end_active_regions: usize,
    changed_total: u64,
    hash: u64,
    /// Peak capacity of the sparse write buffer, in elements (u32 index +
    /// f32 value = 8 bytes each). Zero for dense runs.
    peak_write_slots: usize,
}

fn run_dense_once(rule: Workload, gens: usize) -> RunOutcome {
    let mut s = DenseSession::new(rule, WORLD, WORLD, SEED);
    let t0 = Instant::now();
    let mut changed_total = 0u64;
    for _ in 0..gens {
        changed_total += s.step_generation();
    }
    RunOutcome {
        wall_secs: t0.elapsed().as_secs_f64(),
        events: s.events_total(),
        avg_active_cells: WORLD as f64 * WORLD as f64,
        avg_active_ratio: 1.0,
        end_active_regions: (WORLD as usize) * (WORLD as usize),
        changed_total,
        hash: s.state_hash(),
        peak_write_slots: 0,
    }
}

fn run_sparse_once(rule: Workload, gens: usize) -> RunOutcome {
    let mut s = SparseSession::new(rule, WORLD, WORLD, RS, SEED).unwrap();
    let world_cells = s.world().cell_count();
    let t0 = Instant::now();
    let mut active_sum = 0usize;
    let mut peak_write_slots = 0usize;
    let mut end_active_regions = 0usize;
    for _ in 0..gens {
        let st = s.step_generation().unwrap();
        active_sum += st.active_cells;
        peak_write_slots = peak_write_slots.max(s.scratch_capacity());
        end_active_regions = st.active_regions;
    }
    RunOutcome {
        wall_secs: t0.elapsed().as_secs_f64(),
        events: s.events_total(),
        avg_active_cells: active_sum as f64 / gens as f64,
        avg_active_ratio: active_sum as f64 / (gens * world_cells) as f64,
        end_active_regions,
        changed_total: 0,
        hash: s.state_hash(),
        peak_write_slots,
    }
}

struct Comparison {
    workload: Workload,
    dense: RunOutcome,
    sparse: RunOutcome,
    dense_deterministic: bool,
    sparse_deterministic: bool,
    sparse_matches_dense: bool,
}

fn compare_workload(rule: Workload) -> Comparison {
    // Two runs each: the min wall time is reported (standard best-of-N
    // timing), the hash pair is the determinism check.
    let d1 = run_dense_once(rule, GENS);
    let d2 = run_dense_once(rule, GENS);
    let (dh1, dh2) = (d1.hash, d2.hash);
    let dense_deterministic = dh1 == dh2;
    let dense = if d2.wall_secs < d1.wall_secs { d2 } else { d1 };

    let s1 = run_sparse_once(rule, GENS);
    let s2 = run_sparse_once(rule, GENS);
    let (sh1, sh2) = (s1.hash, s2.hash);
    let sparse_deterministic = sh1 == sh2;
    let sparse = if s2.wall_secs < s1.wall_secs { s2 } else { s1 };

    let sparse_matches_dense = dh1 == dh2 && sh1 == sh2 && dh1 == sh1;

    Comparison {
        workload: rule,
        dense,
        sparse,
        dense_deterministic,
        sparse_deterministic,
        sparse_matches_dense,
    }
}

fn fmt_u64(v: u64) -> String {
    // Thousands separators make the event counts readable.
    let s = v.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(b as char);
    }
    out
}

fn hash_tag(h: u64) -> String {
    format!("{h:016x}")
}

fn run_comparison() {
    println!("=== Part 1: dense synchronous vs sparse event-driven ===");
    println!(
        "world {}x{} ({} cells), regions {}x{}, {} generations, seed {SEED}",
        WORLD,
        WORLD,
        fmt_u64(WORLD as u64 * WORLD as u64),
        WORLD / RS,
        WORLD / RS,
        GENS,
    );
    println!(
        "host: {} {}, logical cores: {:?}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::thread::available_parallelism().map(|n| n.get()).ok(),
    );
    println!();
    println!("Events = cell recomputations (one event per visited cell).");
    println!();

    let mut results = Vec::new();
    for &rule in Workload::ALL.iter() {
        results.push(compare_workload(rule));
    }

    // Kernel baseline for the scheduler-overhead estimate: the cheapest
    // per-cell cost observed across the dense runs (same kernel executes
    // there with essentially zero per-cell scheduling). Using the cheapest
    // observed cost keeps the estimate conservative.
    let kernel_baseline = results
        .iter()
        .map(|c: &Comparison| c.dense.wall_secs / c.dense.events as f64)
        .fold(f64::INFINITY, f64::min);
    println!(
        "kernel baseline (best dense per-cell cost): {:.1} ns — used for the overhead estimates below",
        kernel_baseline * 1e9
    );
    println!();

    // Markdown table — this block is what docs/benchmarks.md embeds.
    println!("| workload | mode | active-cell ratio | events (cell updates) | events/s | wall time / {GENS} gens | scheduler overhead / gen | scratch mem / active cell | final state hash | checks |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");

    for c in &results {
        let checks = format!(
            "det {} / ==dense {}",
            if c.dense_deterministic { "yes" } else { "NO" },
            if c.sparse_matches_dense { "yes" } else { "NO" }
        );
        println!(
            "| {} | dense sync | 100.0% | {} | {:.0} | {:.1} ms | — | 4 B (full-world buffer) | 0x{} | {} |",
            c.workload.name(),
            fmt_u64(c.dense.events),
            c.dense.events as f64 / c.dense.wall_secs,
            c.dense.wall_secs * 1000.0,
            hash_tag(c.dense.hash),
            checks,
        );

        // Sparse scheduler-overhead estimate: sparse wall per generation
        // minus the kernel cost of the cells it visited (baseline per-cell
        // cost from the dense runs). Includes wake/sleep bookkeeping,
        // write buffering and the commit pass. An ESTIMATE by construction.
        let overhead_us = {
            let per_gen = c.sparse.wall_secs / GENS as f64;
            let cellwork = c.sparse.avg_active_cells * kernel_baseline;
            (per_gen - cellwork) * 1e6
        };
        // Sparse scratch: write slots (8 B each) + one active-set entry per
        // region (BTreeSet node, ~64 B est.) over ~1024 cells per region.
        let per_active_cell = if c.sparse.avg_active_cells > 0.0 {
            (c.sparse.peak_write_slots as f64 * 8.0
                + c.sparse.end_active_regions as f64 * 64.0 / 1024.0)
                / c.sparse.avg_active_cells
        } else {
            0.0
        };
        let sparse_checks = format!(
            "det {} / ==dense {}",
            if c.sparse_deterministic { "yes" } else { "NO" },
            if c.sparse_matches_dense { "yes" } else { "NO" }
        );
        println!(
            "| {} | sparse event | {:.1}% | {} | {:.0} | {:.1} ms | {:.0} µs (est.) | {:.1} B | 0x{} | {} |",
            c.workload.name(),
            c.sparse.avg_active_ratio * 100.0,
            fmt_u64(c.sparse.events),
            c.sparse.events as f64 / c.sparse.wall_secs,
            c.sparse.wall_secs * 1000.0,
            overhead_us,
            per_active_cell,
            hash_tag(c.sparse.hash),
            sparse_checks,
        );
    }

    println!();
    // Per-workload settlement detail (the story behind the ratios).
    for c in &results {
        println!(
            "  {}: dense changed-cells total {} | sparse regions awake during last gen: {} / {} | sparse peak write buffer: {} KiB",
            c.workload.name(),
            fmt_u64(c.dense.changed_total),
            c.sparse.end_active_regions,
            (WORLD / RS) * (WORLD / RS),
            c.sparse.peak_write_slots * 8 / 1024,
        );
        let speedup = c.dense.wall_secs / c.sparse.wall_secs;
        println!(
            "    wall-time speedup sparse vs dense: {:.2}x",
            speedup
        );
    }
    println!();
}

// ---------------------------------------------------------------------------
// Part 2: legacy microbenchmark (C++ lineage)
// ---------------------------------------------------------------------------

/// Per-cell work: temperature relaxes toward ambient 0.5.
#[inline]
fn cell_op(v: &mut f32) {
    *v += (0.5 - *v) * 0.01;
}

struct BenchResult {
    label: &'static str,
    visited_cells_per_tick: u64,
    us_per_tick: f64,
}

fn full_update(world: &mut FieldMap, ticks: usize) -> BenchResult {
    let n = world.cell_count();

    // warm-up
    {
        let temp = world.data_mut(Field::Temperature);
        for i in (0..n).step_by(4096) {
            cell_op(&mut temp[i]);
        }
    }

    let t0 = Instant::now();
    for _ in 0..ticks {
        let temp = world.data_mut(Field::Temperature);
        for v in temp.iter_mut() {
            cell_op(v);
        }
    }
    let us_per_tick = t0.elapsed().as_secs_f64() * 1e6 / ticks as f64;

    BenchResult {
        label: "full update (all cells)",
        visited_cells_per_tick: n as u64,
        us_per_tick,
    }
}

fn region_tick(world: &mut FieldMap, sched: &mut RegionScheduler, ticks: usize) -> BenchResult {
    let t0 = Instant::now();
    for _ in 0..ticks {
        sched.tick(world).unwrap();
    }
    let us_per_tick = t0.elapsed().as_secs_f64() * 1e6 / ticks as f64;

    let visited = sched.active_count() as u64 * 32 * 32;
    BenchResult {
        label: "region tick (active only)",
        visited_cells_per_tick: visited,
        us_per_tick,
    }
}

/// Deterministic scatter of `ACTIVE` regions + per-region cell work.
fn setup_sparse_world(sched: &mut RegionScheduler, active: u32) {
    sched.set_idle_ticks_before_sleep(1 << 30); // stay active for the whole run
    for i in 0..active {
        let rx = i.wrapping_mul(2654435761) % sched.regions_x();
        let ry = i.wrapping_mul(40503) % sched.regions_y();
        sched.wake_region(rx, ry).unwrap();
    }

    // Region work: same cell_op over the region's 32x32 cells.
    sched.set_region_work(move |rx, ry, w, _sched| {
        let ww = w.width() as usize;
        let temp = w.data_mut(Field::Temperature);
        let x0 = rx * RS;
        let y0 = ry * RS;
        for y in y0..y0 + RS {
            for x in x0..x0 + RS {
                let idx = y as usize * ww + x as usize;
                cell_op(&mut temp[idx]);
            }
        }
        Ok(())
    });
}

fn run_legacy() {
    println!("=== Part 2 (legacy): region-scheduler microbenchmark ===");

    // ---- Run A: 4096x4096, 16 active regions (0.1%) ----------------------
    {
        const WW: u32 = 4096;
        const WH: u32 = 4096;
        const ACTIVE: u32 = 16;
        let mut world = FieldMap::new(WW, WH);
        let mut sched = RegionScheduler::new(WW, WH, RS).unwrap();
        setup_sparse_world(&mut sched, ACTIVE);

        let full = full_update(&mut world, TICKS);
        let reg = region_tick(&mut world, &mut sched, TICKS);

        println!(
            "=== Run A: world {WW}x{WH}, {ACTIVE} of {} regions active ({:.1}%) ===",
            sched.region_count(),
            100.0 * ACTIVE as f64 / sched.region_count() as f64
        );
        println!(
            "  {} : {:8.1} us/tick  (visits {} cells)",
            full.label, full.us_per_tick, full.visited_cells_per_tick
        );
        println!(
            "  {} : {:8.1} us/tick  (visits {} cells)",
            reg.label, reg.us_per_tick, reg.visited_cells_per_tick
        );
        println!("  speedup     : {:8.1}x", full.us_per_tick / reg.us_per_tick);
        println!(
            "  visited frac: {:.4}% of world",
            100.0 * reg.visited_cells_per_tick as f64 / world.cell_count() as f64
        );
    }

    // ---- Run B: scaling 2048x2048 vs 4096x4096, same 16 active ----------
    {
        const ACTIVE: u32 = 16;
        let run = |ww: u32, wh: u32, label: &str| {
            let mut world = FieldMap::new(ww, wh);
            let mut sched = RegionScheduler::new(ww, wh, RS).unwrap();
            setup_sparse_world(&mut sched, ACTIVE);

            let full = full_update(&mut world, TICKS);
            let reg = region_tick(&mut world, &mut sched, TICKS);
            println!("=== Run B: {label} ===");
            println!(
                "  world {}x{} ({} cells), {ACTIVE}/{} regions active",
                ww,
                wh,
                world.cell_count(),
                sched.region_count()
            );
            println!("  {} : {:8.1} us/tick", full.label, full.us_per_tick);
            println!("  {} : {:8.1} us/tick", reg.label, reg.us_per_tick);
            println!("  speedup     : {:8.1}x", full.us_per_tick / reg.us_per_tick);
            println!();
            // World grew 4x; region tick must stay ~flat (same 16 active
            // regions).
        };
        run(2048, 2048, "2048x2048 (4096 regions)");
        run(4096, 4096, "4096x4096 (16384 regions)");
    }
}

fn main() {
    run_comparison();
    run_legacy();
}

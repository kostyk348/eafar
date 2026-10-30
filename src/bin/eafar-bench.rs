//! S3 core-thesis benchmark — Rust port of `benchmarks/bench_main.cpp`.
//!
//! Thesis: in a sparse world, per-tick cost must be proportional to the
//! ACTIVE area, not the world area. Verified in two ways:
//!
//!   A) Fixed world 4096x4096 (16384 regions), 16 active regions (0.1%):
//!      region-driven tick must be ~1000x cheaper than full update.
//!
//!   B) Scaling: same 16 active regions, world 2048x2048 vs 4096x4096.
//!      Region tick cost must stay ~constant (sublinear in world size),
//!      while full update doubles.
//!
//! The per-cell operation is identical in all cases (temperature decay
//! toward ambient) — the only difference is WHICH cells are visited.

use std::time::Instant;

use eafar::prelude::*;

/// Region edge used by the benchmark (matches the C++ `RS`).
const RS: u32 = 32;
/// Ticks per measurement (matches the C++ `TICKS`).
const TICKS: usize = 20;

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

fn main() {
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

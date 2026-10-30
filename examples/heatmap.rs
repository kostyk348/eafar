//! Generates the promo heatmap: a sparse world where only "hot" regions are awake,
//! outlined over the Temperature field (blue → green → yellow → red ramp).
//!
//! Run: `cargo run --release --example heatmap [out.png]`

use eafar::prelude::*;

fn main() -> Result<()> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "heatmap.png".to_string());
    let (w, h) = (256u32, 256u32);
    let mut world = FieldMap::new(w, h);
    let mut sched = RegionScheduler::new(w, h, 32)?;

    // Three "hot spots" with radial falloff — sparse by construction:
    // most of the world stays asleep and costs nothing to step.
    let spots: [(u32, u32, f32); 3] = [(40, 40, 1.0), (206, 56, 0.8), (116, 210, 0.6)];
    for (cx, cy, amp) in spots {
        for y in 0..h {
            for x in 0..w {
                let dx = x as f32 - cx as f32;
                let dy = y as f32 - cy as f32;
                let d = (dx * dx + dy * dy).sqrt();
                let v = amp * (-d / 14.0).exp();
                if v > 0.0 {
                    let cur = world.get(Field::Temperature, x, y)?;
                    world.set(Field::Temperature, x, y, cur + v)?;
                }
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            if world.get(Field::Temperature, x, y)? > 0.04 {
                sched.wake(x, y)?;
            }
        }
    }

    let sleeping = Visualizer::sleeping_cells(&world, &sched);
    println!(
        "world {}x{}, regions active: {}/{}, sleeping cells: {} ({}%)",
        w,
        h,
        sched.active_count(),
        sched.region_count(),
        sleeping,
        sleeping as f64 * 100.0 / (w as f64 * h as f64)
    );
    Visualizer::export_regions_png(&world, &sched, Field::Temperature, &out, 0.0, 1.0)?;
    println!("written: {}", out);
    Ok(())
}

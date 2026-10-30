//! Rust port of `tests/test_diffusion.cpp` (9 tests).

use std::collections::BTreeSet;

use eafar::prelude::*;

// World 64x64, region 32x32 -> 2x2 regions.

fn seed_center(w: &mut FieldMap, f: Field, v: f32) {
    // A hot/radiating blob in the middle of region (0,0).
    for y in 14..=17u32 {
        for x in 14..=17u32 {
            w.set(f, x, y, v).unwrap();
        }
    }
}

fn sum_field(w: &FieldMap, f: Field) -> f32 {
    let mut s = 0.0f32;
    for y in 0..w.height() {
        for x in 0..w.width() {
            s += w.get(f, x, y).unwrap();
        }
    }
    s
}

#[test]
fn heat_spreads_and_conserves() {
    let mut world = FieldMap::new(64, 64);
    seed_center(&mut world, Field::Temperature, 1.0);
    let before = sum_field(&world, Field::Temperature);

    let eng = DiffusionEngine::default();
    let rule = DiffusionRule {
        kernel: Kernel::VonNeumann,
        rate: 0.1,
        ..Default::default()
    };

    for _ in 0..200 {
        eng.diffuse(&mut world, 0, 0, 32, Field::Temperature, &rule).unwrap();
        eng.diffuse(&mut world, 1, 0, 32, Field::Temperature, &rule).unwrap();
        eng.diffuse(&mut world, 0, 1, 32, Field::Temperature, &rule).unwrap();
        eng.diffuse(&mut world, 1, 1, 32, Field::Temperature, &rule).unwrap();
    }

    let after = sum_field(&world, Field::Temperature);
    // Von Neumann diffusion with no decay conserves total (float drift only).
    check_near!(after, before, 1e-3);

    // Heat spread beyond the source cell.
    check!(world.get(Field::Temperature, 31, 31).unwrap() > 0.0);
    // Center cooled.
    check!(world.get(Field::Temperature, 15, 15).unwrap() < 1.0);
}

#[test]
fn moore_kernel_spreads_diagonally() {
    let mut world = FieldMap::new(64, 64);
    world.set(Field::Temperature, 15, 15, 1.0).unwrap();

    let eng = DiffusionEngine::default();
    let rule = DiffusionRule {
        kernel: Kernel::Moore,
        rate: 0.5,
        ..Default::default()
    };

    eng.diffuse(&mut world, 0, 0, 32, Field::Temperature, &rule).unwrap();

    // Moore reaches diagonal neighbors in one step.
    check!(world.get(Field::Temperature, 16, 16).unwrap() > 0.0);
}

#[test]
fn von_neumann_does_not_spread_diagonal_in_one_step() {
    let mut world = FieldMap::new(64, 64);
    world.set(Field::Temperature, 15, 15, 1.0).unwrap();

    let eng = DiffusionEngine::default();
    let rule = DiffusionRule {
        kernel: Kernel::VonNeumann,
        rate: 0.5,
        ..Default::default()
    };

    eng.diffuse(&mut world, 0, 0, 32, Field::Temperature, &rule).unwrap();

    check_eq!(world.get(Field::Temperature, 16, 16).unwrap(), 0.0);
    check!(world.get(Field::Temperature, 16, 15).unwrap() > 0.0); // cardinal neighbor
}

#[test]
fn diffusion_crosses_region_boundary() {
    // Source at the corner of region (0,0); when the NEIGHBOR region is
    // also active, heat crosses the boundary and updates cells in (1,0).
    let mut world = FieldMap::new(64, 64);
    world.set(Field::Temperature, 31, 31, 1.0).unwrap(); // last cell of region (0,0)

    let eng = DiffusionEngine::default();
    let rule = DiffusionRule {
        kernel: Kernel::VonNeumann,
        rate: 0.5,
        ..Default::default()
    };

    // Region (0,0) reads (31,31) and its neighbors; region (1,0) reads its
    // own cells including the boundary neighbor (31,31) -> (32,31) heats up.
    eng.diffuse(&mut world, 0, 0, 32, Field::Temperature, &rule).unwrap();
    eng.diffuse(&mut world, 1, 0, 32, Field::Temperature, &rule).unwrap();

    check!(world.get(Field::Temperature, 32, 31).unwrap() > 0.0); // in region (1,0)

    // But a sleeping region never changes: region (1,1) not diffused.
    check_eq!(world.get(Field::Temperature, 40, 40).unwrap(), 0.0);
}

#[test]
fn decay_rule_reduces_total() {
    let mut world = FieldMap::new(64, 64);
    seed_center(&mut world, Field::Radiation, 1.0);
    let before = sum_field(&world, Field::Radiation);

    let eng = DiffusionEngine::default();
    let rule = DiffusionRule {
        kernel: Kernel::VonNeumann,
        rate: 0.1,
        decay: 0.02, // radiation decays each step
        ..Default::default()
    };

    for _ in 0..200 {
        eng.diffuse(&mut world, 0, 0, 32, Field::Radiation, &rule).unwrap();
        eng.diffuse(&mut world, 1, 0, 32, Field::Radiation, &rule).unwrap();
        eng.diffuse(&mut world, 0, 1, 32, Field::Radiation, &rule).unwrap();
        eng.diffuse(&mut world, 1, 1, 32, Field::Radiation, &rule).unwrap();
    }

    let after = sum_field(&world, Field::Radiation);
    check!(after < before * 0.5); // decayed significantly
}

#[test]
fn ambient_rule_relaxes_toward_ambient() {
    let mut world = FieldMap::new(64, 64);
    world.set(Field::Humidity, 15, 15, 0.0).unwrap(); // dry spot

    let eng = DiffusionEngine::default();
    let rule = DiffusionRule {
        kernel: Kernel::VonNeumann,
        rate: 0.2,
        ambient: 0.5,
        ..Default::default()
    };

    for _ in 0..50 {
        eng.diffuse(&mut world, 0, 0, 32, Field::Humidity, &rule).unwrap();
    }

    // Dry spot recovers toward ambient, not zero.
    check_near!(world.get(Field::Humidity, 15, 15).unwrap(), 0.5, 0.05);
}

#[test]
fn three_processes_same_engine_simultaneously() {
    // Heat, water and radiation run through ONE engine with different rules.
    let mut world = FieldMap::new(64, 64);
    seed_center(&mut world, Field::Temperature, 1.0);
    seed_center(&mut world, Field::Humidity, 1.0);
    seed_center(&mut world, Field::Radiation, 1.0);

    let heat0 = sum_field(&world, Field::Temperature);
    let water0 = sum_field(&world, Field::Humidity);
    let rad0 = sum_field(&world, Field::Radiation);

    let eng = DiffusionEngine::default();
    let heat = DiffusionRule {
        kernel: Kernel::VonNeumann,
        rate: 0.1,
        ..Default::default()
    };

    let water = DiffusionRule {
        kernel: Kernel::Moore,
        rate: 0.05,
        ..Default::default()
    };

    let rad = DiffusionRule {
        kernel: Kernel::VonNeumann,
        rate: 0.2,
        decay: 0.01,
        ..Default::default()
    };

    for _ in 0..100 {
        for ry in 0..2u32 {
            for rx in 0..2u32 {
                eng.diffuse(&mut world, rx, ry, 32, Field::Temperature, &heat).unwrap();
                eng.diffuse(&mut world, rx, ry, 32, Field::Humidity, &water).unwrap();
                eng.diffuse(&mut world, rx, ry, 32, Field::Radiation, &rad).unwrap();
            }
        }
    }

    // Each process behaves per its own rule:
    // - heat: no decay -> conserved
    // - water: no decay -> conserved (Moore kernel spreads diagonally)
    // - radiation: decays -> total shrinks
    let heat1 = sum_field(&world, Field::Temperature);
    let water1 = sum_field(&world, Field::Humidity);
    let rad1 = sum_field(&world, Field::Radiation);

    check_near!(heat1, heat0, 1e-3);
    check_near!(water1, water0, 1e-3);
    check!(rad1 < rad0 * 0.8);

    // Moore (water) reached the diagonal of the blob; VonNeumann (heat) did
    // not at the same step count... at least verify Moore spread is broader:
    // a far diagonal cell is nonzero for water.
    check!(world.get(Field::Humidity, 18, 18).unwrap() > 0.0); // diagonal of (17,17)
}

#[test]
fn diffuse_active_regions_only() {
    // Only ACTIVE regions get the step; sleeping regions untouched.
    let mut world = FieldMap::new(64, 64);
    world.set(Field::Temperature, 15, 15, 1.0).unwrap(); // region (0,0)
    world.set(Field::Temperature, 40, 40, 1.0).unwrap(); // region (1,1) — stays asleep

    let eng = DiffusionEngine::default();
    let rule = DiffusionRule {
        kernel: Kernel::VonNeumann,
        rate: 0.5,
        ..Default::default()
    };

    let active: BTreeSet<u32> = BTreeSet::from([0]); // only region (0,0)
    eng.diffuse_active(&mut world, &active, 2, 32, Field::Temperature, &rule)
        .unwrap();

    check!(world.get(Field::Temperature, 16, 15).unwrap() > 0.0); // spread in active region
    check_eq!(world.get(Field::Temperature, 40, 40).unwrap(), 1.0); // untouched: asleep
    check_eq!(world.get(Field::Temperature, 41, 40).unwrap(), 0.0);
}

#[test]
fn invalid_rate_throws() {
    let mut world = FieldMap::new(64, 64);
    let eng = DiffusionEngine::default();
    let rule = DiffusionRule {
        rate: 1.5,
        ..Default::default()
    };
    check_throws!(eng.diffuse(&mut world, 0, 0, 32, Field::Temperature, &rule));
}

#[test]
fn out_of_bounds_region_throws() {
    let mut world = FieldMap::new(64, 64);
    let eng = DiffusionEngine::default();
    let rule = DiffusionRule::default();
    check_throws!(eng.diffuse(&mut world, 2, 0, 32, Field::Temperature, &rule));
}

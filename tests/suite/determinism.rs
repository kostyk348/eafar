//! Determinism and replay property tests (audit 2026).
//!
//! Properties under test, for every reference workload:
//!
//! P1. Same seed + same workload + same number of generations → identical
//!     state hash across independent runs (sparse AND dense drivers).
//! P2. The sparse event-driven driver reproduces the dense synchronous
//!     trajectory bit-exactly (same hash) while touching fewer cells.
//! P3. Checkpoint/restore (rewind) followed by re-execution reproduces the
//!     uninterrupted trajectory bit-exactly.
//! P4. Mid-run event injection is reproducible: two runs applying the same
//!     seeded injection schedule get identical hashes, and rewinding before
//!     an injection and replaying it reproduces the no-rewind hash.
//! P5. Different seeds → different hashes (the RNG stream feeds the initial
//!     state, so the difference is deterministic, not probabilistic).
//! P6. Region wake/sleep bookkeeping: a settled world sleeps; an injected
//!     event wakes exactly the containing region.

use eafar::prelude::*;

/// Number of generations used by the property runs (small enough for the
/// test suite, large enough to cross wake/sleep transitions).
const GENS: usize = 120;

fn run_sparse(rule: Workload, seed: u64, gens: usize) -> (u64, u64, FieldMap) {
    let mut s = SparseSession::new(rule, 128, 128, 32, seed).unwrap();
    for _ in 0..gens {
        s.step_generation().unwrap();
    }
    (s.state_hash(), s.events_total(), s.world().clone())
}

fn run_dense(rule: Workload, seed: u64, gens: usize) -> (u64, u64) {
    let mut s = DenseSession::new(rule, 128, 128, seed);
    for _ in 0..gens {
        s.step_generation();
    }
    (s.state_hash(), s.events_total())
}

#[test]
fn p1_sparse_same_seed_identical_hash_across_runs() {
    for &rule in Workload::ALL.iter() {
        for seed in [7u64, 2026_07_31, 0xDEAD_BEEF] {
            let (h1, e1, w1) = run_sparse(rule, seed, GENS);
            let (h2, e2, w2) = run_sparse(rule, seed, GENS);
            assert_eq!(h1, h2, "{rule:?} seed {seed}: sparse hash must repeat");
            assert_eq!(e1, e2, "{rule:?} seed {seed}: event count must repeat");
            assert!(w1.bitwise_equal(&w2), "{rule:?} seed {seed}: worlds bit-equal");
        }
    }
}

#[test]
fn p1_dense_same_seed_identical_hash_across_runs() {
    for &rule in Workload::ALL.iter() {
        let (h1, _) = run_dense(rule, 42, GENS);
        let (h2, _) = run_dense(rule, 42, GENS);
        assert_eq!(h1, h2, "{rule:?}: dense hash must repeat");
    }
}

#[test]
fn p2_sparse_matches_dense_reference() {
    for &rule in Workload::ALL.iter() {
        // Heat reaches exact 0.0 only after ~150 halvings of the largest
        // amplitude, so give it a longer horizon than the other workloads.
        let gens = if rule == Workload::Heat { 200 } else { GENS };
        let (h_dense, e_dense) = run_dense(rule, 99, gens);
        let (h_sparse, e_sparse, _) = run_sparse(rule, 99, gens);
        assert_eq!(
            h_dense, h_sparse,
            "{rule:?}: sparse event-driven trajectory must match the dense reference"
        );
        if matches!(rule, Workload::Heat | Workload::Contagion) {
            // These two settle completely within the horizon, so the
            // sparse driver must have spared actual work.
            assert!(
                e_sparse < e_dense,
                "{rule:?}: sparse events ({e_sparse}) must be < dense events ({e_dense}) on a settling world"
            );
        }
    }
}

#[test]
fn p2b_conway_soup_settles_and_sparse_wins() {
    // A Conway random soup stays chaotic for a few hundred generations and
    // then crystallizes into still lifes and oscillators; after that only
    // the regions holding oscillators stay awake. Given enough generations
    // the sparse driver must have executed strictly fewer events than the
    // dense one (with identical states, as everywhere).
    let mut dense = DenseSession::new(Workload::Conway, 128, 128, 99);
    for _ in 0..500 {
        dense.step_generation();
    }
    let mut sparse = SparseSession::new(Workload::Conway, 128, 128, 32, 99).unwrap();
    for _ in 0..500 {
        sparse.step_generation().unwrap();
    }
    assert_eq!(dense.state_hash(), sparse.state_hash());
    assert!(
        sparse.events_total() < dense.events_total(),
        "settled soup: sparse events ({}) must be < dense events ({})",
        sparse.events_total(),
        dense.events_total()
    );
}

#[test]
fn p3_checkpoint_restore_replay_is_bit_exact() {
    for &rule in Workload::ALL.iter() {
        let mut s = SparseSession::new(rule, 128, 128, 32, 5).unwrap();
        for _ in 0..50 {
            s.step_generation().unwrap();
        }
        let cp = s.checkpoint();
        for _ in 0..70 {
            s.step_generation().unwrap();
        }
        let uninterrupted_hash = s.state_hash();
        let uninterrupted_events = s.events_total();

        // Rewind to generation 50 and redo the same 70 generations.
        s.restore(&cp).unwrap();
        assert_eq!(s.generation(), 50, "{rule:?}: rewind restores the generation counter");
        for _ in 0..70 {
            s.step_generation().unwrap();
        }
        assert_eq!(
            s.state_hash(),
            uninterrupted_hash,
            "{rule:?}: rewind + replay must reproduce the trajectory bit-exactly"
        );
        assert_eq!(s.events_total(), uninterrupted_events);
    }
}

#[test]
fn p4_event_injection_is_reproducible_and_rewindable() {
    // Injection schedule: seeded RNG picks (gen, x, y) heat pokes; sorted
    // by generation so the application order is deterministic.
    let mut rng = SplitMix64::new(4242);
    let mut schedule: Vec<(usize, u32, u32)> = Vec::new();
    for _ in 0..12 {
        let gen = 10 + rng.next_u32(80) as usize;
        let x = rng.next_u32(128);
        let y = rng.next_u32(128);
        schedule.push((gen, x, y));
    }
    schedule.sort();

    // Baseline: straight run with injections.
    let mut s = SparseSession::new(Workload::Heat, 128, 128, 32, 1234).unwrap();
    let mut pending: Vec<(usize, u32, u32)> = schedule.clone();
    for gen in 1..=GENS {
        while let Some(&(g, x, y)) = pending.first() {
            if g == gen {
                s.world_mut().set(Field::Temperature, x, y, 1.0).unwrap();
                s.sched_mut().wake(x, y).unwrap();
                pending.remove(0);
            } else {
                break;
            }
        }
        s.step_generation().unwrap();
    }
    let baseline_hash = s.state_hash();
    let baseline_events = s.events_total();

    // Rewound run: checkpoint at gen 40 (after its injections were
    // applied), rewind, then replay the SAME schedule from gen 41 on.
    let mut s3 = SparseSession::new(Workload::Heat, 128, 128, 32, 1234).unwrap();
    let mut pending3: Vec<(usize, u32, u32)> = schedule.clone();
    let mut cp: Option<SessionCheckpoint> = None;
    for gen in 1..=40 {
        while let Some(&(g, x, y)) = pending3.first() {
            if g == gen {
                s3.world_mut().set(Field::Temperature, x, y, 1.0).unwrap();
                s3.sched_mut().wake(x, y).unwrap();
                pending3.remove(0);
            } else {
                break;
            }
        }
        s3.step_generation().unwrap();
    }
    cp = Some(s3.checkpoint());
    let hash_at_40 = s3.state_hash();

    s3.restore(cp.as_ref().unwrap()).unwrap();
    assert_eq!(s3.generation(), 40);
    assert_eq!(s3.state_hash(), hash_at_40, "restore must reproduce the checkpointed state");

    for gen in 41..=GENS {
        while let Some(&(g, x, y)) = pending3.first() {
            if g == gen {
                s3.world_mut().set(Field::Temperature, x, y, 1.0).unwrap();
                s3.sched_mut().wake(x, y).unwrap();
                pending3.remove(0);
            } else {
                break;
            }
        }
        s3.step_generation().unwrap();
    }

    assert_eq!(
        s3.state_hash(),
        baseline_hash,
        "injected run must be reproducible across independent executions, \
         including after rewind to gen 40 + replay of the remaining injections"
    );
    assert_eq!(s3.events_total(), baseline_events);
}

#[test]
fn p5_different_seeds_diverge() {
    for &rule in Workload::ALL.iter() {
        let (h1, _, _) = run_sparse(rule, 1, 30);
        let (h2, _, _) = run_sparse(rule, 2, 30);
        assert_ne!(h1, h2, "{rule:?}: different seeds must diverge");
    }
}

#[test]
fn p6_settled_world_sleeps_and_events_wake_exactly_one_region() {
    // Heat decays to exact zero: after enough generations the active set
    // must be empty and stay empty.
    let mut s = SparseSession::new(Workload::Heat, 128, 128, 32, 3).unwrap();
    for _ in 0..400 {
        s.step_generation().unwrap();
    }
    assert_eq!(s.sched().active_count(), 0, "settled heat world must sleep");
    let stats = s.step_generation().unwrap();
    assert_eq!(stats.active_cells, 0, "a sleeping world costs zero events");

    // An injected event wakes exactly the region containing the target.
    s.world_mut().set(Field::Temperature, 70, 90, 1.0).unwrap();
    s.sched_mut().wake(70, 90).unwrap();
    assert_eq!(s.sched().active_count(), 1);
    let stats = s.step_generation().unwrap();
    assert!(stats.active_regions >= 1);
    assert!(stats.changed_cells >= 1, "the poke must have propagated");
}

#[test]
fn p7_active_ratio_is_consistent_with_world_size() {
    let mut s = SparseSession::new(Workload::Conway, 128, 128, 32, 11).unwrap();
    let stats = s.step_generation().unwrap();
    let world_cells = s.world().cell_count();
    assert_eq!(world_cells, 128 * 128);
    let ratio = stats.active_ratio(world_cells);
    assert!(ratio > 0.0 && ratio <= 1.0);
    // A fresh random soup wakes essentially the whole world.
    assert!(ratio > 0.5, "initial soup should wake most regions, got {ratio}");
    // And the sparse driver must never claim more events than the world has.
    assert!(stats.cell_updates <= world_cells);
}

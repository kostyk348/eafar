# EAFAR

### Update only what changed.

**EAFAR — Event-Driven Asynchronous Fuzzy Automata Runtime — a zero-dependency Rust library and
a design paradigm for large sparse worlds: state lives in SoA fields, only changed regions wake
up, behavior is automata + events + fuzziness, and runs are bit-exact reproducible artifacts.**

[**English**](README.md) · [Русский](README.ru.md)

---

## What is EAFAR?

Three principles, one small crate, no dependencies:

| Principle | Meaning | EAFAR mechanism |
| --- | --- | --- |
| **Everything is a field** (SoA) | State lives in typed columnar fields, not per-object components | 6 SoA `f32` fields (Temperature, Humidity, Energy, Radiation, Population, Corruption) |
| **Update only what changed** | Inactive regions sleep; only dirty regions step | 32×32 regions with idle aging, deterministic ascending-id iteration |
| **Behavior = automata + events + fuzziness** | Cascades, async clocks, smooth membership transitions | `EventQueue` → `RegionScheduler` → `AutomatonManager` → `FuzzyState` → `DiffusionEngine` |

### The key insight: sparsity is free

Classical grid simulations update a 4096×4096 world every tick — 16.8M cells of wasted work.
EAFAR's region scheduler sleeps dormant regions; the tick cost stays flat as the world grows,
because the **active set** drives the cost, not the world size. Measured with the shipped bench
(`cargo run --release --bin eafar-bench`):

| world | active regions | full update | region tick | speedup |
| --- | --- | --- | --- | --- |
| 4096×4096 | 16 / 16 384 (0.1%) | 3018.8 µs/tick | 6.3 µs/tick | **478×** |
| 2048×2048 | 16 / 4 096 | 722.4 µs/tick | 6.2 µs/tick | **117×** |

| The sparse world (outlined = awake) | Tests + bench (real output) |
| --- | --- |
| ![heatmap](assets/promo/heatmap.png) | ![bench](assets/promo/bench.png) |

## Building blocks (S1–S10)

| Module | File | What it does |
| --- | --- | --- |
| S1 `FieldMap` | `src/field_map.rs` | 6 SoA float fields, bounded access, `bitwise_equal` |
| S2 `EventQueue` | `src/event_queue.rs` | FIFO queue, per-type handlers, cascades (Lightning → Fire → Smoke → Plants), seq numbers |
| S3 `RegionScheduler` | `src/region_scheduler.rs` | 32×32 regions, sleep/wake with idle aging, deterministic ascending-id iteration |
| S4 `AutomatonManager` | `src/automaton.rs` | Asynchronous automata, per-automaton period (Fire@60Hz … Evolution@1h) |
| S5 `FuzzyState` | `src/fuzzy.rs` | Membership functions (step/ramp/triangle/bell) — swap behavior without editing code |
| S6 `DependencyGraph` | `src/dependency.rs` | Declares automaton→field access, structural validation, no automaton→automaton edges |
| S7 `DiffusionEngine` | `src/diffusion.rs` | Single engine, per-process rules (kernel/rate/decay/ambient), cross-boundary reads |
| S8 `WorldSnapshot` | `src/snapshot.rs` | **Bit-exact** capture/restore — NaN patterns and `-0.0` survive rollback (`f32::to_bits`) |
| S9 `SplitMix64` | `src/rng.rs` | Deterministic replay: same seed → same history (bit-identical to the C++ original) |
| S10 `Visualizer` | `src/visualizer.rs` | Dependency-free PNG/CSV heatmap export, region overlay, sleeping-cell stats |

```
use eafar::prelude::*;                     // everything above, one import
```

## Determinism

Same seed + same event sequence → **bit-identical** history and world state. Snapshots restore
bit-exact (including NaN bit patterns). Replay after rollback reproduces the identical final
state. This is what makes EAFAR suitable for scientific use: runs are reproducible artifacts.
The port is tested against the original: SplitMix64 reference vectors and the C++ contract
suite are part of the 78 Rust tests.

## Quick start

```
cargo build --release
cargo test                                        # 78 contract tests
cargo run --release --example heatmap heat.png    # the sparse-world picture above
cargo run --release --bin eafar-bench             # the numbers above
```

```rust
use eafar::prelude::*;

let mut world = FieldMap::new(1024, 1024);
let mut sched = RegionScheduler::new(1024, 1024, 32)?;
sched.wake(500, 500)?;                            // only this region will step
```

## Generalization layers (built on the core)

| Domain instantiation | What maps to | Core building block |
| --- | --- | --- |
| **Spatial simulation** | the original target | all of the above |
| **GUI / UI** | dirty-region rendering = region scheduler; undo = snapshot/rollback; UI tests = replay | `WorldSnapshot`, `EventQueue` |
| **Database** (→ EAFAR-DB) | columnar SoA storage, sparse pages = sleeping regions, materialized views = automata, transactions = snapshot/rollback, audit = replay | `FieldMap`, `WorldSnapshot` |
| **Games** | sparse open worlds (most of the map sleeps), deterministic lockstep (same seed → same state), event cascades as game logic | all of the above, `SplitMix64` |

The core library is **domain-free** — no GUI, no DB engine, no game logic. Those are *layers*
built on the core.

## From C++ to Rust

EAFAR v0.1 was a C++23 library (still in git history). v1.0.0 is the same runtime as a pure
Rust crate: edition 2021, **zero dependencies**, `#![forbid(unsafe_code)]`, `Result` instead of
exceptions, `BTreeSet` for the deterministic iteration order the C++ code guaranteed by
convention. The contract suite was ported 1:1 and extended (78 tests vs 72+), including
bit-exact rollback and SplitMix64 reference vectors.

## Scope

**In (core library):** spatial state, event-driven scheduling, async automata, fuzzy logic,
diffusion, dependency analysis, deterministic replay, visualization exports.

**Out of the core (by design):** NPCs, inventory, ECS, game logic, AI, scripting, GUI, OpenGL —
none of this ships *inside* the runtime. The core is domain-free so that it can serve any
domain; GUI, databases, and game logic are layers built on the core.

## Releases

| Version | What | When |
| --- | --- | --- |
| **v1.0.0** | Rust rewrite: zero-dependency crate, 78 tests, bench + CI ported; C++ superseded | 2026-10-30 |
| **v0.1.0** | C++23 first snapshot: S1–S10, 72+ contract tests, CMake + MinGW script | 2026-10-16 |

## Limitations (honest ones)

See [LIMITATIONS.md](LIMITATIONS.md). Notably: single-threaded ticks, `f32` fields with
strict float ordering (no fast-math shortcuts), stored-deflate PNG export (uncompressed), and
a fixed 8-event type set in `EventType`.

## Roadmap

* ✅ S1–S10 core (C++ → Rust)
* ⬜ EAFAR-DB: columnar storage layer with sparse pages and materialized views
* ⬜ Optional parallel region ticks (behind a feature flag — the core stays dependency-free)
* ⬜ Deterministic networked lockstep helper (seed + event log exchange)
* ⬜ GUI layer demo: dirty-region rendering on top of `RegionScheduler`

## Contributing

PRs welcome — see [CONTRIBUTING.md](CONTRIBUTING.md). Changes are tracked in
[CHANGELOG.md](CHANGELOG.md).

## License

Apache-2.0 — see [LICENSE](LICENSE).

**EAFAR** — the world sleeps; the change wakes it.

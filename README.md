# EAFAR

[![C++23](https://img.shields.io/badge/language-C%2B%2B23-blue)]()
[![CMake](https://img.shields.io/badge/build-CMake-005F73)]()
[![Tests](https://img.shields.io/badge/tests-72%2B%20green-brightgreen)]()
[![License](https://img.shields.io/badge/license-Apache--2.0-red)]()
[![Paradigm](https://img.shields.io/badge/paradigm-sparse--world--simulation-orange)]()

> **EAFAR** — Event-Driven Asynchronous Fuzzy Automata Runtime. A C++23 library and **paradigm** for simulating large sparse worlds where only changed regions wake up.

## About

EAFAR is both a C++23 library and a **design paradigm** for simulation systems. Its three core principles generalize far beyond spatial simulation:

| Principle | Meaning | EAFAR mechanism |
|---|---|---|
| **Everything is a field** (SoA) | State lives in typed columnar fields, not per-object components | 6 SoA float fields (Temperature, Humidity, Energy, Radiation, Population, Corruption) |
| **Update only what changed** | Inactive regions sleep; only dirty regions step | 32×32 regions with idle aging, deterministic ascending-id iteration |
| **Behavior = automata + events + fuzziness** | Cascades, async clocks, smooth membership transitions | `EventQueue` → `RegionScheduler` → `AutomatonManager` → `FuzzyState` → `DiffusionEngine` |

### The key insight: sparsity is free

Classical grid simulations update a 4096×4096 world every tick — 16.8M cells of wasted work. EAFAR's region scheduler sleeps dormant regions with ~3000× speedup, and tick cost stays flat as the world grows (active set drives cost, not world size).

### Generalization layers (built on the core)

| Domain instantiation | What maps to | EAFAR core building block |
|---|---|---|
| **Spatial simulation** | the original target | all of the above |
| **GUI / UI** | dirty-region rendering = region scheduler; undo = snapshot/rollback; UI tests = replay | `WorldSnapshot`, `EventQueue` |
| **Database** (→ EAFAR-DB) | columnar SoA storage, sparse pages = sleeping regions, materialized views = automata, transactions = snapshot/rollback, audit = replay | `FieldMap`, `Snapshot` + EAFAR-DB layer |
| **Games** | sparse open worlds (most of map sleeps), deterministic lockstep (same seed → same state), event cascades as game logic | all of the above, `RNG` for determinism |

The core library is **domain-free** — no GUI, no DB engine, no game logic. Those are *layers* built on the core.

### Topics / tags

`eafar` · `simulation` · `sparse-world` · `soa` · `event-driven` · `fuzzy-logic` · `automata` · `deterministic-replay` · `snapshot-rollback` · `region-scheduler` · `diffusion` · `c++23` · `cmake` · `game-engine` · `paradigm` · `sparse` · `sleeping-regions` · `field-of-arrays` · `determinism` · `reproducibility`

EAFAR is a **paradigm** — a new way to build systems, not only a simulator:

1. **Everything is a field.** State lives in typed, structured fields (SoA),
   separated from behavior. Data and logic are decoupled by design.
2. **Update only what changed.** The world is never recomputed wholesale —
   only regions with changes wake up (dirty-region thinking).
3. **Behavior is automata + events + fuzziness.** Cascades, asynchronous
   clocks, smooth membership transitions, determinism, and replay.

These three principles generalize beyond spatial simulation. The library is
the raw foundation; domain instantiations are built on top of it:

| Layer | Paradigm application |
|---|---|
| **Spatial simulation** | the original target — sparse worlds, event cascades |
| **GUI** | dirty-region rendering = region scheduler; UI state = fields; undo = snapshot/rollback; UI tests = deterministic replay |
| **Database** | columnar (SoA) storage, sparse pages = sleeping regions, **materialized views as automata** (recomputed only when source fields change), transactions = snapshot/rollback, audit = replay |
| **Games** | sparse open worlds (most of the map sleeps), deterministic lockstep networking (same seed → same state), event cascades as game logic |

The core library stays domain-free — no GUI, no DB engine, no game logic
inside the runtime. Those are *layers* built on the core, each an
instantiation of the paradigm.

## Why it exists

Classical grid simulations update the whole world every tick. In a 4096×4096
world with most regions dormant, that is ~16.8M cells of wasted work per tick.
EAFAR's region scheduler sleeps inactive regions and only steps regions that
have pending events or active automata — with a measured **~3000× speedup**
on a realistic sparse workload, and a tick cost that stays flat as the world
grows (the active set, not the world size, drives the cost).

## Building blocks (S1–S10)

| Module | Header | What it does |
|---|---|---|
| S1 FieldMap | `field_map.hpp` | 6 SoA float fields (Temperature, Humidity, Energy, Radiation, Population, Corruption), bounded access |
| S2 EventQueue | `event_queue.hpp` | FIFO queue, per-type handlers, cascades (Lightning → Fire → Smoke → Plants), seq numbers |
| S3 RegionScheduler | `region_scheduler.hpp` | 32×32 regions, sleep/wake with idle aging, deterministic ascending-id iteration |
| S4 AutomatonManager | `automaton.hpp` | Asynchronous automata, per-automaton period (Fire@60Hz … Evolution@1h) |
| S5 FuzzyState | `fuzzy.hpp` | Membership functions (step/ramp/triangle/bell) — swap behavior without editing code |
| S6 DependencyGraph | `dependency.hpp` | Declares automaton→field access, structural validation, no automaton→automaton edges |
| S7 DiffusionEngine | `diffusion.hpp` | Single engine, per-process rules (kernel/rate/decay/ambient), cross-boundary reads, energy conservation |
| S8 WorldSnapshot | `snapshot.hpp` | Bit-exact capture/restore — NaN patterns and -0.0 survive rollback |
| S9 RNG | `rng.hpp` | SplitMix64, deterministic replay: same seed → same history (cross-validated against an independent Python implementation) |
| S10 Visualizer | `visualizer.hpp` | Dependency-free PNG/CSV heatmap export, region overlay, sleeping-cell stats |

## Determinism

Same seed + same event sequence → bit-identical history and world state.
Snapshots restore bit-exact (including NaN bit patterns). Replay after
rollback reproduces the identical final state. This is what makes EAFAR
suitable for scientific use: runs are reproducible artifacts.

## Build

Requires a C++23 compiler (GCC 13+/Clang 16+/MSVC 17.6+) and CMake ≥ 3.20.

```sh
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build
ctest --test-dir build --output-on-failure
```

Or, on Windows with MinGW:

```powershell
./build.ps1
```

## Tests

72+ contract tests, one per spec scenario criterion, all green
(`eafar_tests.exe`). Benchmarks in `benchmarks/bench_main.cpp`
(`eafar_bench.exe`).

## Scope

**In (core library):** spatial state, event-driven scheduling, async automata,
fuzzy logic, diffusion, dependency analysis, deterministic replay,
visualization exports.

**Out of the core (by design):** NPCs, inventory, ECS, game logic, AI,
scripting, GUI, OpenGL — none of this ships *inside* the runtime. The core is
domain-free so that it can serve any domain. GUI, databases, and game logic
are **layers built on the core** (see Paradigm above), not library code.

EAFAR is a simulation *runtime* and a paradigm substrate — application logic
lives in automata you write against the field API.

## License

[Apache 2.0](LICENSE) — see LICENSE file.

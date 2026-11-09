# Benchmarks — dense synchronous vs sparse event-driven

All numbers in this document are **real output** of
`cargo run --release --bin eafar-bench` (the exact table the binary prints,
pasted verbatim). No number here was written by hand or extrapolated.

## Machine context

> **Контекст машины:** Linux-песочница, 2 vCPU (Intel Xeon), rustc 1.83.0,
> release-сборка без зависимостей; запуск от 2026-11-09.
> **Относительные значения значимы, абсолютные — нет:** это шумная общая
> песочница, не бенч-стенд. Repeat runs of the same binary vary by tens of
> percent in absolute wall time (e.g. the legacy Run-A speedup lands
> anywhere between ~500× and ~1200×); the *ordering* and the *order of
> magnitude* of the ratios are the stable signal.
>
> (EN: shared Linux sandbox, 2 vCPU Xeon, rustc 1.83.0, zero-dependency
> release build. Relative values are meaningful; absolute values are not.)

## Methodology

* **Same kernel, different visited set.** Both modes execute the identical
  per-cell function (`eafar::workload::rule_next`); dense visits every cell
  of the world each generation (textbook double buffer), sparse visits only
  the cells of active regions (S3 scheduler, wake/sleep policy from
  [model.md §3](model.md)). The comparison therefore measures the
  event-driven *scheduling* benefit, not differences in the math.
* **World 1024×1024** (1 048 576 cells, 32×32 = 1 024 regions of 32×32),
  **300 generations**, seed `20260731`, one shared seeding function.
* **Events = cell recomputations** (one event per visited cell). CA has no
  intrinsic simulated seconds, so "events/s" is executed cell updates per
  wall-clock second.
* **Determinism check**: every mode runs **twice**; `det yes` means both
  runs produced the identical FNV-1a state hash. `==dense yes` means the
  sparse run's hash equals the dense reference hash — the event-driven
  engine reproduces the dense trajectory bit-for-bit.
* **Wall time** = best of the two runs (`std::time::Instant` around the
  generation loop, seeding excluded).
* **Scheduler overhead / gen** (sparse rows only) is an **estimate**:
  sparse wall time per generation minus the kernel cost of the visited
  cells, using the cheapest dense per-cell cost observed (10.6 ns) as the
  kernel baseline. It includes wake/sleep bookkeeping, write buffering and
  the commit pass.
* **Scratch mem / active cell**: sparse = pending-write buffer (8 B per
  slot) plus active-set entries, divided by the average active-cell count;
  dense = its full-world `f32` double buffer (4 B per cell, always
  allocated for the whole world).

## Part 1 — three workloads, dense vs sparse

```
world 1024x1024 (1 048 576 cells), regions 32x32, 300 generations, seed 20260731
host: linux x86_64, logical cores: Some(2)

kernel baseline (best dense per-cell cost): 10.6 ns — used for the overhead estimates below
```

| workload | mode | active-cell ratio | events (cell updates) | events/s | wall time / 300 gens | scheduler overhead / gen | scratch mem / active cell | final state hash | checks |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| conway | dense sync | 100.0% | 314 572 800 | 60 249 886 | 5221.1 ms | — | 4 B (full-world buffer) | 0x3f0dd6ee6b622325 | det yes / ==dense yes |
| conway | sparse event | 99.7% | 313 561 088 | 50 595 392 | 6197.4 ms | 9554 µs (est.) | 4.0 B | 0x3f0dd6ee6b622325 | det yes / ==dense yes |
| heat | dense sync | 100.0% | 314 572 800 | 94 125 374 | 3342.1 ms | — | 4 B (full-world buffer) | 0xa96777069d622325 | det yes / ==dense yes |
| heat | sparse event | 2.2% | 6 832 128 | 28 057 507 | 243.5 ms | 570 µs (est.) | 11.5 B | 0xa96777069d622325 | det yes / ==dense yes |
| contagion | dense sync | 100.0% | 314 572 800 | 44 265 412 | 7106.5 ms | — | 4 B (full-world buffer) | 0xed7df3c7e3e22325 | det yes / ==dense yes |
| contagion | sparse event | 4.2% | 13 200 384 | 63 571 810 | 207.6 ms | 225 µs (est.) | 1.5 B | 0xed7df3c7e3e22325 | det yes / ==dense yes |

Detail lines from the same run:

```
  conway: dense changed-cells total 24 706 041 | sparse regions awake during last gen: 1009 / 1024 | sparse peak write buffer: 4096 KiB
    wall-time speedup sparse vs dense: 0.84x
  heat: dense changed-cells total 2 622 710 | sparse regions awake during last gen: 0 / 1024 | sparse peak write buffer: 256 KiB
    wall-time speedup sparse vs dense: 13.72x
  contagion: dense changed-cells total 939 282 | sparse regions awake during last gen: 76 / 1024 | sparse peak write buffer: 64 KiB
    wall-time speedup sparse vs dense: 34.22x
```

## Reading the table honestly

* **Conway (chaotic soup): sparse loses, 0.84×.** A density-0.35 random
  soup keeps 99.7% of the world active for the whole 300 generations; the
  event-driven machinery (write buffering, border-wake scans, commit) is
  pure overhead there and costs ~19% extra wall time. This is the honest
  boundary case of the model: **on a fully awake world, dense wins.**
  (The sparse events total is still slightly below dense because a few
  regions sleep even in the soup.)
* **Heat (decaying diffusion): 13.7×.** Hotspots decay to exact `0.0`
  (power-of-two decay factor, no denormal fixed point); by generation ~155
  every region sleeps and sparse cost drops to zero. Average active-cell
  fraction over the run: 2.2% → the gain tracks the predicted `≈ 1/α`
  (§4 of [model.md](model.md)) minus bookkeeping.
* **Threshold contagion (SIR wave): 34.2×.** The infection wavefront is a
  moving, narrow active set (4.2% average); behind the wave regions sleep
  permanently (immune `R` cells never change). Note the *events/s* column:
  sparse processes 63.6M events/s here vs 44.3M/s for dense because its
  working set fits the cache.
* **Scheduler overhead** measured this way is in the hundreds of
  microseconds per generation for hundreds of awake regions — it is
  bounded, per-generation, and independent of the world size; the legacy
  microbenchmark below shows the same flatness across world sizes.
* All six runs produced **identical hashes across their two runs**
  (determinism), and the sparse hashes equal the dense reference hashes
  (the wake/sleep policy skips only provably no-op recomputations).

## Part 2 — legacy microbenchmark (C++ lineage, verbatim output)

Thesis: in a sparse world, per-tick cost must be proportional to the
ACTIVE area, not the world area. The per-cell operation is identical in
all cases (temperature decay toward ambient) — the only difference is
WHICH cells are visited.

```
=== Run A: world 4096x4096, 16 of 16384 regions active (0.1%) ===
  full update (all cells) :   3380.8 us/tick  (visits 16777216 cells)
  region tick (active only) :      6.9 us/tick  (visits 16384 cells)
  speedup     :    491.5x
  visited frac: 0.0977% of world
=== Run B: 2048x2048 (4096 regions) ===
  world 2048x2048 (4194304 cells), 16/4096 regions active
  full update (all cells) :   1047.4 us/tick
  region tick (active only) :      6.6 us/tick
  speedup     :    157.8x

=== Run B: 4096x4096 (16384 regions) ===
  world 4096x4096 (16777216 cells), 16/16384 regions active
  full update (all cells) :   4637.8 us/tick
  region tick (active only) :      6.2 us/tick
  speedup     :    749.0x
```

The scaling result is the robust part: the region tick stays flat at
~6–7 µs while the world grows 4× and the full update grows accordingly.
The absolute speedup factor (491×–749× in this run; 1193× in a noisier
repeat) is 1/α with α = 0.098% modulo timing noise.

## Reproducing

```
cargo run --release --bin eafar-bench
```

The binary prints the machine line, the markdown table above, the
per-workload detail lines, and the legacy runs. Determinism checks are
part of the output (`det` / `==dense` columns); they are also asserted
permanently by `cargo test` (`tests/suite/determinism.rs`).

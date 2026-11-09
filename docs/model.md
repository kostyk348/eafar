# The EAFAR model — a formal description

This document formalizes what EAFAR actually implements, states the cost
model behind its core thesis, and — importantly — states exactly what the
fuzzy layer is and is not. Every claim here is backed either by code in
`src/` or by a measurement in [benchmarks.md](benchmarks.md); anything not
yet implemented is listed as such.

Language note: the README is bilingual (English + Russian); this document
follows the English README.

---

## 1. World and state

A world is a finite rectangular grid `W = {0..width-1} × {0..height-1}` of
cells. State is **SoA** (structure of arrays): for each semantic field
`f ∈ F = {Temperature, Humidity, Energy, Radiation, Population, Corruption}`
there is one contiguous `f32` array `x_f: W → f32` ([`FieldMap`],
`src/field_map.rs`). There are no per-cell objects; a cell is an index
shared by all fields.

Two properties of the storage matter for everything below:

* **Bit-exactness.** State comparisons and snapshots operate on `f32::to_bits`
  (`bitwise_equal`, `WorldSnapshot`), so NaN payloads and `-0.0` are part of
  the state, not noise.
* **Fixed iteration order.** Every traversal in the runtime is row-major or
  ascending-id (`BTreeSet`), never hash order.

## 2. Dense synchronous CA (the reference semantics)

The classical model EAFAR is measured against: given an update rule `R`
(where `R` for cell `c` reads the values of `c`'s neighbors in the
**previous** state),

```
x_{t+1}(c) = R( x_t | N(c) )    for ALL c ∈ W
```

This is a *synchronous* (Jacobi) update: all cells read the same `x_t`.
Cost is `Θ(|W|)` per generation, regardless of how much of the world
actually changed. [`DenseSession`](`src/workload.rs`) implements exactly
this (double buffer, shared per-cell kernel).

## 3. Sparse event-driven CA and the active set

EAFAR partitions `W` into `s × s` regions (default `s = 32`,
[`RegionScheduler`], `src/region_scheduler.rs`) and maintains the
**active set**

```
A_t ⊆ {0..regions_x-1} × {0..regions_y-1}
```

a set of regions. Generation `t` recomputes the rule only for cells inside
`⋃_{r ∈ A_t} r`, still reading the previous state (so the semantics of the
update are the same synchronous rule as in §2 — only the visited set is
smaller). Writes are buffered per generation and committed after the whole
active set has been processed (`SparseSession::step_generation`).

The active set evolves by a **wake/sleep lifecycle**, driven by events and
local change detection:

1. **Wake on event.** An external event (injected state change, queued
   `EventQueue` cascade, user click in the visualizer) wakes the region
   containing the target cell: `A_t ← A_t ∪ {region(e)}`.
2. **Stay awake on change.** While processing generation `t`, a region that
   produced at least one changed cell re-wakes itself: it stays in
   `A_{t+1}`. (In scheduler terms: `wake_region` during the work phase
   marks the region `woken_during_work`, which resets its idle counter.)
3. **Wake the neighborhood of change.** A changed cell on a region border
   wakes every region containing a cell within Moore distance 1 of the
   change — those regions' inputs changed. Interior changes cannot reach
   another region's input set (a cell's inputs lie within distance 1), so
   they wake nobody. The wake radius is deliberately the Moore ring for
   all rules, which is conservative for von Neumann kernels.
4. **Sleep on idle.** A region that ran and did not change and was not
   re-woken ages past `idle_ticks_before_sleep` (default 1) and leaves
   `A_{t+1}`. Sleeping regions cost zero.

Formally, with `Δ_t = {c ∈ W : x_{t+1}(c) ≠ x_t(c)}` (bit-exact
comparison) and `ring(r) = {c : dist(c, r) ≤ 1}`:

```
A_{t+1} = { r : ring(r) ∩ Δ_t ≠ ∅ }  ∪  wakes_t
```

where `wakes_t` are the regions woken by external events during `t`. The
correctness argument is short: the rule's input for cell `c` is
`x_t | N(c)` with `N(c)` of radius ≤ 1; if no cell in `N(c)` changed, the
recomputation of `c` is a no-op, so skipping regions whose `ring` did not
change reproduces the dense trajectory. This is verified as a property
test (`p2_sparse_matches_dense_reference` in `tests/suite/determinism.rs`)
and as a `==dense` cross-check in every benchmark run: sparse and dense
**state hashes are equal bit-for-bit**.

## 4. Cost model: potential gain as a function of the active fraction

Let `α_t = |⋃_{r ∈ A_t} r| / |W|` be the **active-cell fraction** (the
"active-cell ratio" reported by the benchmark), `w` the per-cell kernel
cost, and `c_reg` the per-active-region scheduler cost (iteration, wake/sleep
bookkeeping, commit). Then per generation:

```
T_dense  = |W| · w                          + 0
T_sparse = α_t · |W| · w + |A_t| · c_reg    + (write buffering + commit ≈ O(|A_t|·s²))
```

and the **potential gain** — the upper bound of the sparse advantage — is

```
G(α) = T_dense / T_sparse  ≈  1 / ( α + (c_reg / (s² · w)) · α )  →  1/α   as c_reg → 0
```

i.e. the gain is the inverse active fraction, degraded by scheduler
overhead. Two boundary cases follow immediately, and both are measured in
[benchmarks.md](benchmarks.md):

* **α → 1** (nothing sleeps, e.g. a chaotic Conway soup): `G < 1` — the
  event machinery is pure overhead. Measured: **0.84×** on a random soup
  that keeps 99.7% of cells active (the machinery costs ~19% wall time).
* **α → small** (the world settles): `G ≈ 1/α`. Measured: heat settles to
  2.2% average active fraction → **13.7×**; a contagion wavefront keeps
  4.2% → **34.2×**; 16 of 16 384 regions active (0.098% of cells) →
  **~500–1200×** in the legacy microbenchmark (the spread is sandbox
  timing noise; the region-tick cost itself is flat at ~6–7 µs).

The honest summary: **sparse event-driven execution never beats dense on a
fully awake world; its advantage is exactly `1/α` of the active fraction
the workload actually needs, minus bookkeeping.** Any performance claim for
a specific domain must therefore be prefixed by the workload's active
fraction and the [scheduler benchmark](benchmarks.md).

### The locality / frame-size trade-off

The region edge `s` (the "frame size") controls how finely the active set
can track change:

| effect | smaller `s` | larger `s` |
| --- | --- | --- |
| wasted work inside an active region | less (region hugs the change) | more (whole `s×s` frame steps for one changed corner) |
| scheduler bookkeeping | more regions, more `BTreeSet` ops, more wake/sleep churn | fewer regions, cheaper bookkeeping |
| boundary effects | more region borders → more halo reads and border-wake events per changed cell | fewer borders, better locality |
| granularity of `A_t` | fine (activity tracked per small frame) | coarse (one oscillator keeps a huge frame awake) |

EAFAR fixes `s = 32` (1 024 cells per region): large enough that the
per-region overhead (~tens of ns) is negligible against 1 024 kernel
invocations (~µs), small enough that a single still-life cluster does not
pin down a large frame. `s` is a constructor parameter
(`RegionScheduler::new(w, h, s)`); the trade-off above is the reason it is
not smaller. A systematic sweep over `s` is future work (see
[LIMITATIONS.md](../LIMITATIONS.md)).

## 5. Determinism

Determinism is a design invariant, not a hope:

* iteration is always ascending region id (`BTreeSet`) and row-major
  within regions;
* the PRNG is a fixed SplitMix64 (S9); same seed → same stream, verified
  against published reference vectors;
* `f32` operations run in a fixed order (no reassociation, no fast-math);
  rule constants are pre-computed so operation order is fixed by
  construction;
* snapshots (`WorldSnapshot`, S8) capture/restore bit-exactly, including
  NaN payloads and `-0.0`;
* **state hash** = FNV-1a 64 over the `f32` bit patterns of a field,
  row-major (`hash_field`). Property tests (`tests/suite/determinism.rs`)
  assert: same seed + workload → identical hash across runs (P1);
  sparse == dense (P2); checkpoint/restore + replay == uninterrupted run
  (P3, P4); different seeds → different hashes (P5).

Replay support: the engine supports true rewind via `SparseSession::checkpoint()`
/ `restore()` — a checkpoint is a `WorldSnapshot` (all six fields,
bit-exact) **plus the active region set and the event/generation counters**;
continuing after a restore reproduces the uninterrupted trajectory
bit-for-bit (property P3). A plain `WorldSnapshot` restores fields only —
the active set is session state and is restored by the session, not by the
snapshot type. This is stated explicitly because "snapshot = full rewind"
would otherwise be an overclaim.

## 6. The fuzzy layer — what exists, and what does not

EAFAR's name contains "Fuzzy". Here is exactly what that means in code
([`FuzzyState` module](../src/fuzzy.rs), S5) — no more, no less.

**What exists:**

* **Membership functions** — `type MembershipFn = Arc<dyn Fn(f32) -> f32>`,
  mapping a raw field value to a degree in `[0, 1]`. Four built-in shapes
  (namespace `mf`):
  * `step(threshold)` — sharp threshold;
  * `ramp(lo, hi)` — linear rise between `lo` and `hi`;
  * `triangle(center, width)` — triangular peak;
  * `bell(center, spread)` — Cauchy-like bell `1/(1+r²)` (deterministic,
    no libm).
* **A named registry** — `FuzzyRegistry::define(name, f)` / `apply(name, v)`
  / `has` / `count`. Automata reference memberships by name, so behavior is
  swappable without editing code (that is the S5 design point).
* **`clamp01`** — clamping into the fuzzy domain.

**What does NOT exist (honestly):**

* **t-norms** (AND) and **t-conorms / s-norms** (OR);
* **fuzzy implication** operators;
* **rule bases and inference** (no Mamdani, no Sugeno, no rule evaluation
  engine, no aggregation of rule outputs);
* **defuzzification** (no centroid, no mean-of-maxima, nothing).

So the "fuzzy" in EAFAR 1.x is a **pluggable membership/thresholding
layer**: it turns raw field values into degrees and lets behavior depend on
those degrees by name. It is not a fuzzy inference system. Consistent with
that, this repository makes **no performance claims about fuzzy
inference**, because there is no fuzzy inference to measure; and any
future claim of the form "fuzzy evaluation costs X" must first define
which operators were added and benchmarked (see the honesty section in the
[README](../README.md)).

## 7. Concept → module map

| Concept | Code |
| --- | --- |
| SoA fields `x_f` | `src/field_map.rs` (S1), `src/field.rs` |
| Event-driven change propagation | `src/event_queue.rs` (S2), cascades |
| Regions, active set `A_t`, wake/sleep lifecycle | `src/region_scheduler.rs` (S3) |
| Multi-cadence updates per active region | `src/automaton.rs` (S4) |
| Fuzzy layer (membership functions + registry only) | `src/fuzzy.rs` (S5) |
| Declared field coupling (no automaton→automaton edges) | `src/dependency.rs` (S6) |
| Diffusion processes per active region | `src/diffusion.rs` (S7) |
| Bit-exact snapshots / rewind | `src/snapshot.rs` (S8), `SessionCheckpoint` in `src/workload.rs` |
| Deterministic PRNG | `src/rng.rs` (S9) |
| PNG/CSV export | `src/visualizer.rs` (S10) |
| Reference rules + dense/sparse drivers + state hash | `src/workload.rs` |
| Interactive visualizer (GUI demo) | `viz/` (workspace crate, egui/eframe) |
| Metric tables, dense vs sparse comparison | `src/bin/eafar-bench.rs`, [benchmarks.md](benchmarks.md) |

[`DenseSession`]: ../src/workload.rs
[`RegionScheduler`]: ../src/region_scheduler.rs
[`FieldMap`]: ../src/field_map.rs

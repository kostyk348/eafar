# Launch posts for EAFAR (EN)

## Hacker News (Show HN)

**Show HN: EAFAR — event-driven fuzzy automata runtime, rewritten in zero-dependency Rust**

EAFAR is a small runtime and a design paradigm for large sparse worlds. Three principles:

1. *Everything is a field* — state lives in typed SoA columns (Temperature, Humidity,
   Energy, Radiation, Population, Corruption), decoupled from behavior.
2. *Update only what changed* — the world is divided into 32×32 regions; dormant regions
   sleep with idle aging, and the tick cost follows the active set, not the world size.
   On a 4096×4096 world with 0.1% of regions active the shipped bench measures 478×
   speedup (3018 µs → 6.3 µs per tick).
3. *Behavior is automata + events + fuzziness* — cascades (Lightning → Fire → Smoke →
   Plants), per-automaton periods (Fire@60Hz … Evolution@1h), membership functions you can
   swap without touching code.

The whole point is determinism as an engineering requirement: same seed + same events →
bit-identical history; snapshots restore bit-exact via `f32::to_bits` (NaN payloads and
-0.0 survive); iteration order is ascending id (`BTreeSet`); SplitMix64 is checked against
the original C++ test vectors. Zero dependencies, `#![forbid(unsafe_code)]`, 78 contract
tests (the C++ suite ported 1:1 and extended), own PNG writer for heatmap export.

v0.1 was C++23; v1.0.0 is the same runtime in pure Rust — the port is documented, the C++
lives in git history.

https://github.com/kostyk348/eafar

## Reddit r/rust (текст поста)

I rewrote my simulation runtime from C++23 to Rust with a hard constraint: zero
dependencies (the PNG writer, the RNG, everything is in-tree). The interesting parts for
r/rust: using `BTreeSet`/`BTreeMap` everywhere to make iteration order part of the API
contract; `#![forbid(unsafe_code)]` for a numerics-heavy crate; `Result` instead of
exceptions while keeping C++ error strings; and porting a contract test suite 1:1
(including bit-exact rollback tests with NaN payloads). Bench: a 4096×4096 world with
0.1% active regions steps in 6.3 µs vs 3018 µs for the naive full update. Feedback
welcome, especially on the determinism guarantees.

## Post for X/Twitter

EAFAR 1.0: event-driven fuzzy automata runtime, now in zero-dependency Rust.
Sparse worlds sleep; only changed regions step. 478× on a 4096×4096 world
with 0.1% active. Bit-exact replays. 78 tests.
https://github.com/kostyk348/eafar

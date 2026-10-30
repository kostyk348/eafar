//! Deterministic PRNG.
//!
//! C++ lineage: `include/eafar/rng.hpp` (S9).
//!
//! Same seed → same sequence, bit-exact, across runs and platforms.
//! `rand`/default PRNGs are NOT used: their implementation is not
//! guaranteed to be stable. SplitMix64 is a tiny fixed algorithm (standard
//! reference implementation, Vigna 2014).

/// SplitMix64 — fixed, portable, deterministic PRNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

/// Golden-ratio constant used by SplitMix64.
const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

impl SplitMix64 {
    /// Create a generator from a seed.
    pub fn new(seed: u64) -> Self {
        SplitMix64 { state: seed }
    }

    /// Next raw 64-bit value. Bit-identical to the C++ `next()`.
    pub fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, bound)`. `bound` must be `> 0`.
    pub fn next_bounded(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    /// Uniform `u32` in `[0, bound)` (C++ `next_u32`).
    pub fn next_u32(&mut self, bound: u32) -> u32 {
        (self.next() % bound as u64) as u32
    }

    /// Uniform float in `[0, 1)` — deterministic bit pattern.
    ///
    /// Top 24 bits of a 64-bit value → exactly representable float.
    pub fn next_unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / 16777216.0
    }

    /// Reset the state (C++ `seed`).
    pub fn seed(&mut self, seed: u64) {
        self.state = seed;
    }
}

impl Default for SplitMix64 {
    fn default() -> Self {
        SplitMix64::new(0)
    }
}

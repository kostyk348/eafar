#pragma once
// eafar/rng.hpp — S9: Deterministic PRNG.
//
// Same seed -> same sequence, bit-exact, across runs, platforms and
// compilers. std::rand / <random> default engines are NOT used: their
// implementation is not guaranteed to be stable. SplitMix64 is a tiny
// fixed algorithm (standard reference implementation).

#include <cstdint>

namespace eafar {

class SplitMix64 {
public:
    explicit SplitMix64(std::uint64_t seed) noexcept : state_(seed) {}

    std::uint64_t next() noexcept {
        std::uint64_t z = (state_ += 0x9E3779B97F4A7C15ull);
        z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ull;
        z = (z ^ (z >> 27)) * 0x94D049BB133111EBull;
        return z ^ (z >> 31);
    }

    // Uniform in [0, bound). bound must be > 0.
    std::uint64_t next_bounded(std::uint64_t bound) noexcept {
        return next() % bound;
    }

    std::uint32_t next_u32(std::uint32_t bound) noexcept {
        return static_cast<std::uint32_t>(next() % bound);
    }

    // Uniform float in [0, 1) — deterministic bit pattern.
    float next_unit() noexcept {
        // Top 24 bits of a 64-bit value -> exactly representable float.
        return static_cast<float>(next() >> 40) / 16777216.0f;
    }

    void seed(std::uint64_t seed) noexcept { state_ = seed; }

private:
    std::uint64_t state_;
};

} // namespace eafar

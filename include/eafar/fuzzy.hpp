#pragma once
// eafar/fuzzy.hpp — S5: Fuzzy State System.
//
// No bools — everything is a float membership in [0..1]. A membership
// function maps a raw field value to a degree of "the predicate holds"
// (e.g. ignition(x) = how flammable a Temperature reading is).
//
// Membership functions are pluggable and swapped WITHOUT editing the
// automaton that consumes them: automata depend on a named membership,
// the runtime resolves it.

#include <cstdint>
#include <functional>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <utility>

namespace eafar {

// A membership function: raw value -> membership degree in [0,1].
using MembershipFn = std::function<float(float)>;

// Common built-in shapes.
namespace mf {

// Step: 0 below `threshold`, 1 at/above. Sharp, fuzzy-free.
inline MembershipFn step(float threshold) {
    return [threshold](float v) -> float { return v >= threshold ? 1.0f : 0.0f; };
}

// Ramp: 0 at/below lo, 1 at/above hi, linear between.
inline MembershipFn ramp(float lo, float hi) {
    return [lo, hi](float v) -> float {
        if (v <= lo) return 0.0f;
        if (v >= hi) return 1.0f;
        return (v - lo) / (hi - lo);
    };
}

// Triangle: rises to 1 at `center`, 0 at center +/- width.
inline MembershipFn triangle(float center, float width) {
    return [center, width](float v) -> float {
        const float d = (v > center) ? (v - center) : (center - v);
        if (d >= width) return 0.0f;
        return 1.0f - d / width;
    };
}

// Gaussian-ish bell (deterministic, no <cmath> needed).
inline MembershipFn bell(float center, float spread) {
    return [center, spread](float v) -> float {
        const float d = (v > center) ? (v - center) : (center - v);
        const float r = d / spread;
        return 1.0f / (1.0f + r * r);
    };
}

} // namespace mf

// Named membership registry. Automata reference memberships by name,
// so swapping the function changes behavior without touching automata.
class FuzzyRegistry {
public:
    void define(std::string name, MembershipFn fn) {
        if (fn == nullptr) {
            throw std::invalid_argument("eafar: membership fn is null");
        }
        fns_[std::move(name)] = std::move(fn);
    }

    float apply(const std::string& name, float raw) const {
        auto it = fns_.find(name);
        if (it == fns_.end()) {
            throw std::out_of_range("eafar: unknown membership '" + name + "'");
        }
        return it->second(raw);
    }

    bool has(const std::string& name) const noexcept {
        return fns_.find(name) != fns_.end();
    }

    std::size_t count() const noexcept { return fns_.size(); }

private:
    std::unordered_map<std::string, MembershipFn> fns_;
};

// Convenience: clamp a raw value into the fuzzy [0,1] domain.
inline float clamp01(float v) noexcept {
    if (v < 0.0f) return 0.0f;
    if (v > 1.0f) return 1.0f;
    return v;
}

} // namespace eafar

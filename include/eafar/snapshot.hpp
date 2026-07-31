#pragma once
// eafar/snapshot.hpp — S8: Snapshot / Rollback.
//
// A Snapshot is a copy of the entire world state (every field array).
// Because the world is SoA, a snapshot is just N contiguous arrays —
// capture and restore are memcpy-equivalent, fast and bit-exact.
//
// Usage: snapshot -> run 10000 events -> restore -> the world is bit-
// identical to the snapshot. This powers debugging, "what if" exploration,
// and (with S9) rollback-then-replay.

#include "eafar/field_map.hpp"

#include <array>
#include <cstdint>
#include <cstring>
#include <vector>

namespace eafar {

class WorldSnapshot {
public:
    WorldSnapshot() = default;

    static WorldSnapshot capture(const FieldMap& world) {
        WorldSnapshot s;
        s.width_ = world.width();
        s.height_ = world.height();
        for (std::size_t i = 0; i < kFieldCount; ++i) {
            const Field f = kAllFields[i];
            const float* src = world.data(f);
            s.fields_[i].assign(src, src + world.cell_count());
        }
        return s;
    }

    // Restore into an existing world. The world must have the same
    // dimensions (checked); arrays are copied bit-exactly.
    void restore(FieldMap& world) const {
        if (world.width() != width_ || world.height() != height_) {
            throw std::invalid_argument(
                "eafar: snapshot dimensions do not match target world");
        }
        for (std::size_t i = 0; i < kFieldCount; ++i) {
            const Field f = kAllFields[i];
            float* dst = world.data(f);
            const auto& src = fields_[i];
            std::memcpy(dst, src.data(), src.size() * sizeof(float));
        }
    }

    bool matches(const FieldMap& world) const noexcept {
        if (world.width() != width_ || world.height() != height_) {
            return false;
        }
        for (std::size_t i = 0; i < kFieldCount; ++i) {
            const Field f = kAllFields[i];
            const float* p = world.data(f);
            const auto& src = fields_[i];
            if (std::memcmp(p, src.data(), src.size() * sizeof(float)) != 0) {
                return false;
            }
        }
        return true;
    }

    std::uint32_t width() const noexcept { return width_; }
    std::uint32_t height() const noexcept { return height_; }

private:
    std::uint32_t width_ = 0;
    std::uint32_t height_ = 0;
    std::array<std::vector<float>, kFieldCount> fields_;
};

} // namespace eafar

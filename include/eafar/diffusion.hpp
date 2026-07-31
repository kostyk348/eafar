#pragma once
// eafar/diffusion.hpp — S7: Diffusion Engine.
//
// ONE infrastructure for every spreading process (heat, water, radiation,
// disease, smell, sound). Only the RULE differs: stencil kernel, transfer
// rate, decay, ambient. A diffusion step runs only inside ACTIVE regions
// (S3 locality) but reads neighbors across region boundaries.
//
// Determinism: row-major iteration, scratch-buffer commit (no in-place
// read-after-write), fixed float ops — same order every run.

#include "eafar/field_map.hpp"

#include <cstdint>
#include <set>

namespace eafar {

enum class Kernel : std::uint8_t { VonNeumann, Moore };

struct DiffusionRule {
    Kernel kernel = Kernel::VonNeumann;
    float rate = 0.1f;   // fraction of neighbor mean transferred per step, [0,1]
    float decay = 0.0f;  // extra per-step decay toward 0 (e.g. radiation)
    float ambient = 0.0f; // if > 0, relax toward this value instead of neighbors
};

class DiffusionEngine {
public:
    // Apply one diffusion step to `field` inside region (rx, ry) of a world
    // partitioned into region_size x region_size regions.
    // Throws std::invalid_argument if rate not in [0,1] or region out of bounds.
    void diffuse(FieldMap& world, std::uint32_t rx, std::uint32_t ry,
                 std::uint32_t region_size, Field field,
                 const DiffusionRule& rule) const;

    // Bulk: run the rule on every ACTIVE region (ascending id), reusing the
    // same scratch buffer. Deterministic.
    void diffuse_active(FieldMap& world,
                        const std::set<std::uint32_t>& active_region_ids,
                        std::uint32_t regions_x, std::uint32_t region_size,
                        Field field, const DiffusionRule& rule) const;

private:
    // Per-cell neighbor mean, honoring world edges. Returns neighbor count used.
    float neighbor_mean(const FieldMap& world, Field field, std::uint32_t x,
                        std::uint32_t y, Kernel kernel, float ambient,
                        std::uint32_t& count) const;
};

} // namespace eafar

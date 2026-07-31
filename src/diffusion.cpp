#include "eafar/diffusion.hpp"

#include <stdexcept>
#include <vector>

namespace eafar {

namespace {

// Region-relative offsets of the 4/8 neighbors.
constexpr std::int32_t kN4dx[4] = {1, -1, 0, 0};
constexpr std::int32_t kN4dy[4] = {0, 0, 1, -1};
constexpr std::int32_t kN8dx[8] = {1, -1, 0, 0, 1, 1, -1, -1};
constexpr std::int32_t kN8dy[8] = {0, 0, 1, -1, 1, -1, 1, -1};

} // namespace

float DiffusionEngine::neighbor_mean(const FieldMap& world, Field field,
                                     std::uint32_t x, std::uint32_t y,
                                     Kernel kernel, float ambient,
                                     std::uint32_t& count) const {
    if (ambient > 0.0f) {
        count = 1;
        return ambient;
    }
    const std::int32_t* dx = (kernel == Kernel::VonNeumann) ? kN4dx : kN8dx;
    const std::int32_t* dy = (kernel == Kernel::VonNeumann) ? kN4dy : kN8dy;
    const int n = (kernel == Kernel::VonNeumann) ? 4 : 8;

    float sum = 0.0f;
    count = 0;
    for (int i = 0; i < n; ++i) {
        const std::int32_t nx = static_cast<std::int32_t>(x) + dx[i];
        const std::int32_t ny = static_cast<std::int32_t>(y) + dy[i];
        if (nx < 0 || ny < 0 ||
            nx >= static_cast<std::int32_t>(world.width()) ||
            ny >= static_cast<std::int32_t>(world.height())) {
            continue; // world edge: neighbor doesn't exist
        }
        sum += world.get(field, static_cast<std::uint32_t>(nx),
                         static_cast<std::uint32_t>(ny));
        ++count;
    }
    return count > 0 ? sum / static_cast<float>(count) : world.get(field, x, y);
}

void DiffusionEngine::diffuse(FieldMap& world, std::uint32_t rx,
                              std::uint32_t ry, std::uint32_t region_size,
                              Field field, const DiffusionRule& rule) const {
    if (rule.rate < 0.0f || rule.rate > 1.0f) {
        throw std::invalid_argument("eafar: diffusion rate must be in [0,1]");
    }
    if (rule.decay < 0.0f || rule.decay > 1.0f) {
        throw std::invalid_argument("eafar: diffusion decay must be in [0,1]");
    }

    const std::uint32_t x0 = rx * region_size;
    const std::uint32_t y0 = ry * region_size;
    if (x0 >= world.width() || y0 >= world.height()) {
        throw std::out_of_range("eafar: diffusion region out of bounds");
    }
    const std::uint32_t x1 =
        std::min(x0 + region_size, static_cast<std::uint32_t>(world.width()));
    const std::uint32_t y1 =
        std::min(y0 + region_size, static_cast<std::uint32_t>(world.height()));

    // Scratch for the new values: commit only after the whole region is read.
    std::vector<float> scratch(static_cast<std::size_t>(x1 - x0) * (y1 - y0));

    for (std::uint32_t y = y0; y < y1; ++y) {
        for (std::uint32_t x = x0; x < x1; ++x) {
            std::uint32_t count = 0;
            const float mean =
                neighbor_mean(world, field, x, y, rule.kernel, rule.ambient, count);
            const float cur = world.get(field, x, y);
            float next = cur + rule.rate * (mean - cur);
            if (rule.decay > 0.0f) {
                next -= rule.decay * next; // decay toward 0
            }
            scratch[static_cast<std::size_t>(y - y0) * (x1 - x0) + (x - x0)] = next;
        }
    }

    for (std::uint32_t y = y0; y < y1; ++y) {
        for (std::uint32_t x = x0; x < x1; ++x) {
            world.set(field, x, y,
                      scratch[static_cast<std::size_t>(y - y0) * (x1 - x0) +
                              (x - x0)]);
        }
    }
}

void DiffusionEngine::diffuse_active(FieldMap& world,
                                     const std::set<std::uint32_t>& active_ids,
                                     std::uint32_t regions_x,
                                     std::uint32_t region_size, Field field,
                                     const DiffusionRule& rule) const {
    for (std::uint32_t id : active_ids) { // ascending (std::set)
        const std::uint32_t rx = id % regions_x;
        const std::uint32_t ry = id / regions_x;
        diffuse(world, rx, ry, region_size, field, rule);
    }
}

} // namespace eafar

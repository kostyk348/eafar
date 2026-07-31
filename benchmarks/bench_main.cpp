// eafar/benchmarks/bench_main.cpp — S3 core-thesis benchmark.
//
// Thesis: in a sparse world, per-tick cost must be proportional to the
// ACTIVE area, not the world area. We verify in two ways:
//
//   A) Fixed world 4096x4096 (16384 regions), 16 active regions (0.1%):
//      region-driven tick must be ~1000x cheaper than full update.
//
//   B) Scaling: same 16 active regions, world 2048x2048 vs 4096x4096.
//      Region tick cost must stay ~constant (sublinear in world size),
//      while full update doubles.
//
// The per-cell operation is identical in all cases (temperature decay
// toward ambient) — the only difference is WHICH cells are visited.

#include "eafar/field_map.hpp"
#include "eafar/region_scheduler.hpp"

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <vector>

using namespace eafar;
using Clock = std::chrono::steady_clock;

// Per-cell work: temperature relaxes toward ambient 0.5.
static inline void cell_op(float& v) {
    v += (0.5f - v) * 0.01f;
}

struct Result {
    const char* label;
    std::uint64_t world_cells;
    std::uint64_t visited_cells_per_tick;
    double us_per_tick;
};

static Result full_update(FieldMap& world, int ticks) {
    float* temp = world.data(Field::Temperature);
    const std::uint64_t n = world.cell_count();

    // warm-up
    for (std::uint64_t i = 0; i < n; i += 4096) {
        cell_op(temp[i]);
    }

    auto t0 = Clock::now();
    for (int t = 0; t < ticks; ++t) {
        for (std::uint64_t i = 0; i < n; ++i) {
            cell_op(temp[i]);
        }
    }
    auto t1 = Clock::now();
    const double us = std::chrono::duration<double, std::micro>(t1 - t0).count() / ticks;

    return Result{"full update (all cells)", n, n, us};
}

static Result region_tick(FieldMap& world, RegionScheduler& sched, int ticks) {
    auto t0 = Clock::now();
    for (int t = 0; t < ticks; ++t) {
        sched.tick(world);
    }
    auto t1 = Clock::now();
    const double us = std::chrono::duration<double, std::micro>(t1 - t0).count() / ticks;

    const std::uint64_t visited =
        static_cast<std::uint64_t>(sched.active_count()) * 32 * 32;
    return Result{"region tick (active only)", world.cell_count(), visited, us};
}

int main() {
    constexpr std::uint32_t RS = 32;
    constexpr int TICKS = 20;

    // ---- Run A: 4096x4096, 16 active regions (0.1%) ----------------------
    {
        constexpr std::uint32_t WW = 4096, WH = 4096;
        FieldMap world(WW, WH);
        RegionScheduler sched(WW, WH, RS);
        sched.set_idle_ticks_before_sleep(1 << 30); // stay active for the whole run

        // Deterministic scatter of 16 active regions across the world.
        constexpr std::uint32_t ACTIVE = 16;
        for (std::uint32_t i = 0; i < ACTIVE; ++i) {
            const std::uint32_t rx = (i * 2654435761u) % sched.regions_x();
            const std::uint32_t ry = (i * 40503u) % sched.regions_y();
            sched.wake_region(rx, ry);
        }

        // Region work: same cell_op over the region's 32x32 cells.
        sched.set_region_work([](std::uint32_t rx, std::uint32_t ry, FieldMap& w) {
            float* temp = w.data(Field::Temperature);
            const std::uint32_t ww = w.width();
            const std::uint32_t x0 = rx * RS, y0 = ry * RS;
            for (std::uint32_t y = y0; y < y0 + RS; ++y) {
                for (std::uint32_t x = x0; x < x0 + RS; ++x) {
                    cell_op(temp[static_cast<std::size_t>(y) * ww + x]);
                }
            }
        });

        Result full = full_update(world, TICKS);
        Result reg = region_tick(world, sched, TICKS);

        std::printf("=== Run A: world %ux%u, %u of %u regions active (%.1f%%) ===\n",
                    WW, WH, ACTIVE, sched.region_count(),
                    100.0 * ACTIVE / sched.region_count());
        std::printf("  full update : %8.1f us/tick  (visits %llu cells)\n",
                    full.us_per_tick, (unsigned long long)full.visited_cells_per_tick);
        std::printf("  region tick : %8.1f us/tick  (visits %llu cells)\n",
                    reg.us_per_tick, (unsigned long long)reg.visited_cells_per_tick);
        std::printf("  speedup     : %8.1fx\n", full.us_per_tick / reg.us_per_tick);
        std::printf("  visited frac: %.4f%% of world\n",
                    100.0 * reg.visited_cells_per_tick / world.cell_count());
    }

    // ---- Run B: scaling 2048x2048 vs 4096x4096, same 16 active ----------
    {
        constexpr std::uint32_t ACTIVE = 16;
        auto run = [&](std::uint32_t WW, std::uint32_t WH, const char* label) {
            FieldMap world(WW, WH);
            RegionScheduler sched(WW, WH, RS);
            sched.set_idle_ticks_before_sleep(1 << 30);
            for (std::uint32_t i = 0; i < ACTIVE; ++i) {
                const std::uint32_t rx = (i * 2654435761u) % sched.regions_x();
                const std::uint32_t ry = (i * 40503u) % sched.regions_y();
                sched.wake_region(rx, ry);
            }
            sched.set_region_work([](std::uint32_t rx, std::uint32_t ry, FieldMap& w) {
                float* temp = w.data(Field::Temperature);
                const std::uint32_t ww = w.width();
                const std::uint32_t x0 = rx * RS, y0 = ry * RS;
                for (std::uint32_t y = y0; y < y0 + RS; ++y) {
                    for (std::uint32_t x = x0; x < x0 + RS; ++x) {
                        cell_op(temp[static_cast<std::size_t>(y) * ww + x]);
                    }
                }
            });

            Result full = full_update(world, TICKS);
            Result reg = region_tick(world, sched, TICKS);
            std::printf("=== Run B: %s ===\n", label);
            std::printf("  world %ux%u (%llu cells), %u/%u regions active\n",
                        WW, WH, (unsigned long long)world.cell_count(),
                        ACTIVE, sched.region_count());
            std::printf("  full update : %8.1f us/tick\n", full.us_per_tick);
            std::printf("  region tick : %8.1f us/tick\n", reg.us_per_tick);
            std::printf("  speedup     : %8.1fx\n", full.us_per_tick / reg.us_per_tick);
            std::printf("\n");
        };
        run(2048, 2048, "2048x2048 (4096 regions)");
        run(4096, 4096, "4096x4096 (16384 regions)");
        // World grew 4x; region tick must stay ~flat (same 16 active regions).
    }

    return 0;
}

#include "test_runner.hpp"
#include "eafar/diffusion.hpp"
#include "eafar/region_scheduler.hpp"

#include <cstdint>

using namespace eafar;

// World 64x64, region 32x32 -> 2x2 regions.

static void seed_center(FieldMap& w, Field f, float v) {
    // A hot/radiating blob in the middle of region (0,0).
    for (std::uint32_t y = 14; y <= 17; ++y) {
        for (std::uint32_t x = 14; x <= 17; ++x) {
            w.set(f, x, y, v);
        }
    }
}

static float sum_field(const FieldMap& w, Field f) {
    float s = 0.0f;
    for (std::uint32_t y = 0; y < w.height(); ++y) {
        for (std::uint32_t x = 0; x < w.width(); ++x) {
            s += w.get(f, x, y);
        }
    }
    return s;
}

TEST(heat_spreads_and_conserves) {
    FieldMap world(64, 64);
    seed_center(world, Field::Temperature, 1.0f);
    const float before = sum_field(world, Field::Temperature);

    DiffusionEngine eng;
    DiffusionRule rule;
    rule.kernel = Kernel::VonNeumann;
    rule.rate = 0.1f;

    for (int t = 0; t < 200; ++t) {
        eng.diffuse(world, 0, 0, 32, Field::Temperature, rule);
        eng.diffuse(world, 1, 0, 32, Field::Temperature, rule);
        eng.diffuse(world, 0, 1, 32, Field::Temperature, rule);
        eng.diffuse(world, 1, 1, 32, Field::Temperature, rule);
    }

    const float after = sum_field(world, Field::Temperature);
    // Von Neumann diffusion with no decay conserves total (float drift only).
    CHECK_NEAR(after, before, 1e-3f);

    // Heat spread beyond the source cell.
    CHECK(world.get(Field::Temperature, 31, 31) > 0.0f);
    // Center cooled.
    CHECK(world.get(Field::Temperature, 15, 15) < 1.0f);
}

TEST(moore_kernel_spreads_diagonally) {
    FieldMap world(64, 64);
    world.set(Field::Temperature, 15, 15, 1.0f);

    DiffusionEngine eng;
    DiffusionRule rule;
    rule.kernel = Kernel::Moore;
    rule.rate = 0.5f;

    eng.diffuse(world, 0, 0, 32, Field::Temperature, rule);

    // Moore reaches diagonal neighbors in one step.
    CHECK(world.get(Field::Temperature, 16, 16) > 0.0f);
}

TEST(von_neumann_does_not_spread_diagonal_in_one_step) {
    FieldMap world(64, 64);
    world.set(Field::Temperature, 15, 15, 1.0f);

    DiffusionEngine eng;
    DiffusionRule rule;
    rule.kernel = Kernel::VonNeumann;
    rule.rate = 0.5f;

    eng.diffuse(world, 0, 0, 32, Field::Temperature, rule);

    CHECK_EQ(world.get(Field::Temperature, 16, 16), 0.0f);
    CHECK(world.get(Field::Temperature, 16, 15) > 0.0f); // cardinal neighbor
}

TEST(diffusion_crosses_region_boundary) {
    // Source at the corner of region (0,0); when the NEIGHBOR region is
    // also active, heat crosses the boundary and updates cells in (1,0).
    FieldMap world(64, 64);
    world.set(Field::Temperature, 31, 31, 1.0f); // last cell of region (0,0)

    DiffusionEngine eng;
    DiffusionRule rule;
    rule.kernel = Kernel::VonNeumann;
    rule.rate = 0.5f;

    // Region (0,0) reads (31,31) and its neighbors; region (1,0) reads its
    // own cells including the boundary neighbor (31,31) -> (32,31) heats up.
    eng.diffuse(world, 0, 0, 32, Field::Temperature, rule);
    eng.diffuse(world, 1, 0, 32, Field::Temperature, rule);

    CHECK(world.get(Field::Temperature, 32, 31) > 0.0f); // in region (1,0)

    // But a sleeping region never changes: region (1,1) not diffused.
    CHECK_EQ(world.get(Field::Temperature, 40, 40), 0.0f);
}

TEST(decay_rule_reduces_total) {
    FieldMap world(64, 64);
    seed_center(world, Field::Radiation, 1.0f);
    const float before = sum_field(world, Field::Radiation);

    DiffusionEngine eng;
    DiffusionRule rule;
    rule.kernel = Kernel::VonNeumann;
    rule.rate = 0.1f;
    rule.decay = 0.02f; // radiation decays each step

    for (int t = 0; t < 200; ++t) {
        eng.diffuse(world, 0, 0, 32, Field::Radiation, rule);
        eng.diffuse(world, 1, 0, 32, Field::Radiation, rule);
        eng.diffuse(world, 0, 1, 32, Field::Radiation, rule);
        eng.diffuse(world, 1, 1, 32, Field::Radiation, rule);
    }

    const float after = sum_field(world, Field::Radiation);
    CHECK(after < before * 0.5f); // decayed significantly
}

TEST(ambient_rule_relaxes_toward_ambient) {
    FieldMap world(64, 64);
    world.set(Field::Humidity, 15, 15, 0.0f); // dry spot

    DiffusionEngine eng;
    DiffusionRule rule;
    rule.kernel = Kernel::VonNeumann;
    rule.rate = 0.2f;
    rule.ambient = 0.5f;

    for (int t = 0; t < 50; ++t) {
        eng.diffuse(world, 0, 0, 32, Field::Humidity, rule);
    }

    // Dry spot recovers toward ambient, not zero.
    CHECK_NEAR(world.get(Field::Humidity, 15, 15), 0.5f, 0.05f);
}

TEST(three_processes_same_engine_simultaneously) {
    // Heat, water and radiation run through ONE engine with different rules.
    FieldMap world(64, 64);
    seed_center(world, Field::Temperature, 1.0f);
    seed_center(world, Field::Humidity, 1.0f);
    seed_center(world, Field::Radiation, 1.0f);

    const float heat0 = sum_field(world, Field::Temperature);
    const float water0 = sum_field(world, Field::Humidity);
    const float rad0 = sum_field(world, Field::Radiation);

    DiffusionEngine eng;
    DiffusionRule heat;
    heat.kernel = Kernel::VonNeumann;
    heat.rate = 0.1f;

    DiffusionRule water;
    water.kernel = Kernel::Moore;
    water.rate = 0.05f;

    DiffusionRule rad;
    rad.kernel = Kernel::VonNeumann;
    rad.rate = 0.2f;
    rad.decay = 0.01f;

    for (int t = 0; t < 100; ++t) {
        for (std::uint32_t ry = 0; ry < 2; ++ry) {
            for (std::uint32_t rx = 0; rx < 2; ++rx) {
                eng.diffuse(world, rx, ry, 32, Field::Temperature, heat);
                eng.diffuse(world, rx, ry, 32, Field::Humidity, water);
                eng.diffuse(world, rx, ry, 32, Field::Radiation, rad);
            }
        }
    }

    // Each process behaves per its own rule:
    // - heat: no decay -> conserved
    // - water: no decay -> conserved (Moore kernel spreads diagonally)
    // - radiation: decays -> total shrinks
    const float heat1 = sum_field(world, Field::Temperature);
    const float water1 = sum_field(world, Field::Humidity);
    const float rad1 = sum_field(world, Field::Radiation);

    CHECK_NEAR(heat1, heat0, 1e-3f);
    CHECK_NEAR(water1, water0, 1e-3f);
    CHECK(rad1 < rad0 * 0.8f);

    // Moore (water) reached the diagonal of the blob; VonNeumann (heat) did
    // not at the same step count... at least verify Moore spread is broader:
    // a far diagonal cell is nonzero for water.
    CHECK(world.get(Field::Humidity, 18, 18) > 0.0f); // diagonal of (17,17)
}

TEST(diffuse_active_regions_only) {
    // Only ACTIVE regions get the step; sleeping regions untouched.
    FieldMap world(64, 64);
    world.set(Field::Temperature, 15, 15, 1.0f);     // region (0,0)
    world.set(Field::Temperature, 40, 40, 1.0f);     // region (1,1) — stays asleep

    DiffusionEngine eng;
    DiffusionRule rule;
    rule.kernel = Kernel::VonNeumann;
    rule.rate = 0.5f;

    std::set<std::uint32_t> active = {0}; // only region (0,0)
    eng.diffuse_active(world, active, 2, 32, Field::Temperature, rule);

    CHECK(world.get(Field::Temperature, 16, 15) > 0.0f);  // spread in active region
    CHECK_EQ(world.get(Field::Temperature, 40, 40), 1.0f); // untouched: asleep
    CHECK_EQ(world.get(Field::Temperature, 41, 40), 0.0f);
}

TEST(invalid_rate_throws) {
    FieldMap world(64, 64);
    DiffusionEngine eng;
    DiffusionRule rule;
    rule.rate = 1.5f;
    CHECK_THROWS(eng.diffuse(world, 0, 0, 32, Field::Temperature, rule));
}

TEST(out_of_bounds_region_throws) {
    FieldMap world(64, 64);
    DiffusionEngine eng;
    DiffusionRule rule;
    CHECK_THROWS(eng.diffuse(world, 2, 0, 32, Field::Temperature, rule));
}

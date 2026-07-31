#include "test_runner.hpp"
#include "eafar/region_scheduler.hpp"
#include "eafar/event_queue.hpp"

#include <cstdint>

using namespace eafar;

// 128x128 world, 32x32 regions -> 4x4 = 16 regions.
static constexpr std::uint32_t W = 128;
static constexpr std::uint32_t H = 128;
static constexpr std::uint32_t RS = 32;

static std::uint32_t rx_of(std::uint32_t wx) { return wx / RS; }
static std::uint32_t ry_of(std::uint32_t wy) { return wy / RS; }

TEST(region_geometry) {
    RegionScheduler s(W, H, RS);
    CHECK_EQ(s.region_size(), RS);
    CHECK_EQ(s.regions_x(), 4u);
    CHECK_EQ(s.regions_y(), 4u);
    CHECK_EQ(s.region_count(), 16u);

    // world coord (40, 70) -> region (1, 2) -> id 2*4+1 = 9
    CHECK_EQ(s.region_x(40), 1u);
    CHECK_EQ(s.region_y(70), 2u);
    CHECK_EQ(s.region_id(40, 70), 9u);
}

TEST(new_scheduler_all_regions_sleep) {
    RegionScheduler s(W, H, RS);
    CHECK_EQ(s.active_count(), 0u);
    CHECK(s.active_ids().empty());
}

TEST(event_wakes_only_its_region) {
    FieldMap world(W, H);
    RegionScheduler s(W, H, RS);
    s.set_idle_ticks_before_sleep(5);

    // Event at (40, 40) -> region (1, 1) -> id 5
    s.wake(40, 40);
    CHECK_EQ(s.active_count(), 1u);
    CHECK(s.is_active(rx_of(40), ry_of(40)));
    CHECK(s.is_active_id(5));

    // Neighbors stay asleep.
    CHECK(!s.is_active_id(1));
    CHECK(!s.is_active_id(4));
    CHECK(!s.is_active_id(6));
    CHECK(!s.is_active_id(9));
    CHECK(!s.is_active(0, 0));
}

TEST(region_sleeps_after_idle_ticks) {
    FieldMap world(W, H);
    RegionScheduler s(W, H, RS);
    s.set_idle_ticks_before_sleep(2);

    s.wake(40, 40);
    CHECK_EQ(s.active_count(), 1u);

    s.tick(world); // idle 1
    CHECK_EQ(s.active_count(), 1u);
    s.tick(world); // idle 2 -> sleep
    CHECK_EQ(s.active_count(), 0u);
}

TEST(work_runs_only_on_active_regions) {
    FieldMap world(W, H);
    RegionScheduler s(W, H, RS);

    int work_calls = 0;
    std::uint32_t last_rx = 999, last_ry = 999;
    s.set_region_work([&](std::uint32_t rx, std::uint32_t ry, FieldMap&) {
        ++work_calls;
        last_rx = rx;
        last_ry = ry;
    });

    s.wake(40, 40); // region (1,1)
    s.tick(world);

    CHECK_EQ(work_calls, 1);
    CHECK_EQ(last_rx, 1u);
    CHECK_EQ(last_ry, 1u);

    // Idle region produces zero work.
    s.tick(world);
    CHECK_EQ(work_calls, 1);
}

TEST(work_wakes_neighbor_region) {
    FieldMap world(W, H);
    RegionScheduler s(W, H, RS);
    s.set_idle_ticks_before_sleep(3);

    // Region (1,1) work wakes region (0,0) (e.g. event spread).
    s.set_region_work([&](std::uint32_t rx, std::uint32_t ry, FieldMap&) {
        if (rx == 1 && ry == 1) {
            s.wake_region(0, 0);
        }
    });

    s.wake_region(1, 1);
    s.tick(world);

    CHECK(s.is_active(1, 1));
    CHECK(s.is_active(0, 0));
    CHECK_EQ(s.active_count(), 2u);
}

TEST(re_woken_region_gets_fresh_lifetime) {
    FieldMap world(W, H);
    RegionScheduler s(W, H, RS);
    s.set_idle_ticks_before_sleep(1);

    // Work re-wakes its own region every tick (persistent activity).
    s.set_region_work([&](std::uint32_t rx, std::uint32_t ry, FieldMap&) {
        s.wake_region(rx, ry);
    });

    s.wake(40, 40);
    for (int i = 0; i < 10; ++i) {
        s.tick(world);
        CHECK_EQ(s.active_count(), 1u); // never sleeps while re-woken
    }
}

TEST(sleep_is_deterministic_order) {
    FieldMap world(W, H);
    RegionScheduler s(W, H, RS);
    s.set_idle_ticks_before_sleep(5);

    // Wake in scrambled order; iteration order must be ascending id.
    s.wake(100, 100); // region (3,3) -> id 15
    s.wake(10, 10);   // region (0,0) -> id 0
    s.wake(70, 20);   // region (2,0) -> id 2

    std::vector<std::uint32_t> ids(s.active_ids().begin(), s.active_ids().end());
    CHECK_EQ(ids.size(), 3u);
    CHECK_EQ(ids[0], 0u);
    CHECK_EQ(ids[1], 2u);
    CHECK_EQ(ids[2], 15u);
}

TEST(out_of_bounds_wake_throws) {
    RegionScheduler s(W, H, RS);
    CHECK_THROWS(s.wake_region(4, 0));
    CHECK_THROWS(s.wake_region(0, 4));
    CHECK_THROWS(s.wake_region(99, 99));
}

TEST(zero_region_size_throws) {
    bool threw = false;
    try {
        RegionScheduler s(W, H, 0);
    } catch (...) {
        threw = true;
    }
    CHECK(threw);
}

TEST(world_not_multiple_of_region_size) {
    // 100x100 world with 32x32 regions -> 4x4 (ceil) with partial edges.
    RegionScheduler s(100, 100, RS);
    CHECK_EQ(s.regions_x(), 4u);
    CHECK_EQ(s.regions_y(), 4u);
    CHECK_EQ(s.region_count(), 16u);
    // Last world coord maps into the last region.
    CHECK_EQ(s.region_x(99), 3u);
    CHECK_EQ(s.region_y(99), 3u);
}

#include "test_runner.hpp"
#include "eafar/snapshot.hpp"
#include "eafar/event_queue.hpp"
#include "eafar/automaton.hpp"
#include "eafar/region_scheduler.hpp"
#include "eafar/diffusion.hpp"

#include <bit>
#include <cmath>
#include <cstdint>

using namespace eafar;

// ---------------------------------------------------------------------------
// S8 spec scenario: snapshot -> 10000 events -> rollback -> bit-exact match.
// ---------------------------------------------------------------------------

TEST(snapshot_rollback_after_10000_events) {
    FieldMap world(64, 64);
    RegionScheduler sched(world.width(), world.height(), 32);
    sched.set_idle_ticks_before_sleep(1 << 30);

    EventQueue q;
    q.subscribe(EventType::Lightning, [](const Event& e, FieldMap& w, EventQueue& q) {
        w.set(Field::Temperature, e.x, e.y, w.get(Field::Temperature, e.x, e.y) + 0.50f);
        q.push(EventType::Fire, e.x, e.y, 0.90f);
    });
    q.subscribe(EventType::Fire, [](const Event& e, FieldMap& w, EventQueue& q) {
        w.set(Field::Energy, e.x, e.y, 0.05f);
        q.push(EventType::Smoke, e.x, e.y, 0.70f);
    });
    q.subscribe(EventType::Smoke, [](const Event& e, FieldMap& w, EventQueue& q) {
        w.set(Field::Population, e.x, e.y, w.get(Field::Population, e.x, e.y) - 0.40f);
        q.push(EventType::Plants, e.x, e.y, 0.30f);
    });
    q.subscribe(EventType::Plants, [](const Event& e, FieldMap& w, EventQueue&) {
        w.set(Field::Energy, e.x, e.y, 0.40f);
    });

    // The deterministic strike pattern used by both runs.
    auto strike = [](EventQueue& qq) {
        for (int i = 0; i < 10000; ++i) {
            const std::uint32_t x = static_cast<std::uint32_t>((i * 2654435761u) % 64);
            const std::uint32_t y = static_cast<std::uint32_t>((i * 40503u) % 64);
            qq.push(EventType::Lightning, x, y, 1.0f);
        }
    };

    // Run 1: from pristine world.
    strike(q);
    q.process_all(world);
    sched.tick(world);
    CHECK_EQ(q.processed_count(), 40000u); // 4 events per strike
    WorldSnapshot run1_final = WorldSnapshot::capture(world);

    // Rollback to pristine.
    WorldSnapshot pristine = WorldSnapshot::capture(FieldMap(64, 64)); // all-zero
    WorldSnapshot run1_final_snap = WorldSnapshot::capture(world);     // == run1_final
    pristine.restore(world);
    CHECK(world.bitwise_equal(FieldMap(64, 64))); // bit-exact rollback to zero
    (void)run1_final_snap;

    // Run 2: same deterministic pattern from the rolled-back state.
    EventQueue q2;
    q2.subscribe(EventType::Lightning, [](const Event& e, FieldMap& w, EventQueue& q) {
        w.set(Field::Temperature, e.x, e.y, w.get(Field::Temperature, e.x, e.y) + 0.50f);
        q.push(EventType::Fire, e.x, e.y, 0.90f);
    });
    q2.subscribe(EventType::Fire, [](const Event& e, FieldMap& w, EventQueue& q) {
        w.set(Field::Energy, e.x, e.y, 0.05f);
        q.push(EventType::Smoke, e.x, e.y, 0.70f);
    });
    q2.subscribe(EventType::Smoke, [](const Event& e, FieldMap& w, EventQueue& q) {
        w.set(Field::Population, e.x, e.y, w.get(Field::Population, e.x, e.y) - 0.40f);
        q.push(EventType::Plants, e.x, e.y, 0.30f);
    });
    q2.subscribe(EventType::Plants, [](const Event& e, FieldMap& w, EventQueue&) {
        w.set(Field::Energy, e.x, e.y, 0.40f);
    });
    strike(q2);
    q2.process_all(world);
    sched.tick(world);

    // Rollback-then-redo reproduces the exact same final world (bit-exact).
    CHECK(run1_final.matches(world));
    CHECK(world.bitwise_equal(FieldMap(64, 64)) == false); // world is not zero
    (void)pristine;
}

TEST(snapshot_matches_after_restore) {
    FieldMap world(32, 32);
    world.set(Field::Temperature, 5, 5, 0.77f);
    world.set(Field::Corruption, 30, 30, 0.01f);

    WorldSnapshot snap = WorldSnapshot::capture(world);
    CHECK(snap.matches(world));

    // Mutate wildly.
    world.fill(Field::Radiation, 1.0f);
    world.set(Field::Temperature, 5, 5, -3.0f);
    CHECK(!snap.matches(world));

    snap.restore(world);
    CHECK(snap.matches(world));
    CHECK_EQ(world.get(Field::Temperature, 5, 5), 0.77f);
    CHECK_EQ(world.get(Field::Corruption, 30, 30), 0.01f);
    CHECK_EQ(world.get(Field::Radiation, 0, 0), 0.0f); // restored to zero
}

TEST(snapshot_is_bit_exact) {
    // Bit-exact: NaN bit patterns and -0.0 survive restore byte-for-byte.
    FieldMap world(8, 8);
    const float neg_zero = -0.0f;
    const float nan_bits = std::bit_cast<float>(std::uint32_t{0x7FC00001u});
    world.set(Field::Energy, 1, 1, neg_zero);
    world.set(Field::Energy, 2, 2, nan_bits);

    WorldSnapshot snap = WorldSnapshot::capture(world);
    world.set(Field::Energy, 1, 1, 123.0f);
    world.set(Field::Energy, 2, 2, 0.0f);

    snap.restore(world);

    // -0.0 == 0.0 in IEEE but must be restored as -0.0 (bit pattern).
    CHECK(std::signbit(world.get(Field::Energy, 1, 1)));
    // NaN compare with itself is false; check the bit pattern instead.
    CHECK_EQ(std::bit_cast<std::uint32_t>(world.get(Field::Energy, 2, 2)),
             std::uint32_t{0x7FC00001u});
}

TEST(snapshot_dimension_mismatch_throws) {
    FieldMap small(8, 8);
    FieldMap big(16, 16);
    WorldSnapshot snap = WorldSnapshot::capture(small);
    CHECK_THROWS(snap.restore(big));
}

TEST(snapshot_captures_all_six_fields) {
    FieldMap world(4, 4);
    for (std::size_t i = 0; i < kFieldCount; ++i) {
        world.set(kAllFields[i], 0, 0, 0.1f * static_cast<float>(i + 1));
    }
    WorldSnapshot snap = WorldSnapshot::capture(world);

    for (std::size_t i = 0; i < kFieldCount; ++i) {
        world.set(kAllFields[i], 0, 0, 9.0f);
    }
    snap.restore(world);

    for (std::size_t i = 0; i < kFieldCount; ++i) {
        CHECK_EQ(world.get(kAllFields[i], 0, 0), 0.1f * static_cast<float>(i + 1));
    }
}

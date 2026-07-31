#include "test_runner.hpp"
#include "eafar/rng.hpp"
#include "eafar/event_queue.hpp"
#include "eafar/snapshot.hpp"
#include "eafar/region_scheduler.hpp"

#include <cstdint>
#include <string>
#include <vector>

using namespace eafar;

// ---------------------------------------------------------------------------
// S9 spec: same seed -> identical history (diff = 0).
// ---------------------------------------------------------------------------

// A full deterministic simulation: RNG-driven lightning strikes on a world.
struct SimOutcome {
    std::vector<Event> history; // every processed event, in order
    FieldMap world;             // final state
    std::uint64_t processed;
};

static SimOutcome run_deterministic_sim(std::uint64_t seed,
                                        std::uint32_t strikes) {
    FieldMap world(64, 64);
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

    SplitMix64 rng(seed);
    for (std::uint32_t i = 0; i < strikes; ++i) {
        const std::uint32_t x = rng.next_u32(64);
        const std::uint32_t y = rng.next_u32(64);
        q.push(EventType::Lightning, x, y, 1.0f);
        q.process_all(world);
    }

    return SimOutcome{q.history(), std::move(world), q.processed_count()};
}

TEST(same_seed_identical_history) {
    SimOutcome a = run_deterministic_sim(12345, 1000);
    SimOutcome b = run_deterministic_sim(12345, 1000);

    // Histories are identical: same length, same events, same seq numbers.
    CHECK_EQ(a.history.size(), b.history.size());
    CHECK_EQ(a.history.size(), 4000u); // 1000 strikes * 4-event cascade
    CHECK_EQ(a.processed, b.processed);

    for (std::size_t i = 0; i < a.history.size(); ++i) {
        const Event& ea = a.history[i];
        const Event& eb = b.history[i];
        CHECK(ea.type == eb.type);
        CHECK(ea.x == eb.x);
        CHECK(ea.y == eb.y);
        CHECK(ea.payload == eb.payload);
        CHECK(ea.seq == eb.seq);
    }

    // Final world states are bit-exact.
    CHECK(a.world.bitwise_equal(b.world));
}

TEST(different_seed_different_history) {
    SimOutcome a = run_deterministic_sim(1, 500);
    SimOutcome b = run_deterministic_sim(2, 500);

    // Same structure, but the RNG stream differs -> strike positions differ.
    bool any_difference = false;
    const std::size_t n = a.history.size() < b.history.size() ? a.history.size()
                                                              : b.history.size();
    for (std::size_t i = 0; i < n; ++i) {
        if (a.history[i].x != b.history[i].x ||
            a.history[i].y != b.history[i].y) {
            any_difference = true;
            break;
        }
    }
    CHECK(any_difference);
    CHECK(!a.world.bitwise_equal(b.world));
}

TEST(rng_sequence_is_stable_and_known) {
    // SplitMix64 canonical algorithm, seed 0. Reference values computed by
    // an independent Python implementation of the published algorithm
    // (Vigna, 2014): seed0 -> e220a8397b1dcdaf, 6e789e6aa1b965f4,
    // 6c45d188009454f, f88bb8a8724c81ec.
    SplitMix64 r(0);
    const std::uint64_t expected[] = {
        0xE220A8397B1DCDAFull, 0x6E789E6AA1B965F4ull,
        0x06C45D188009454Full, 0xF88BB8A8724C81ECull};
    for (std::uint64_t e : expected) {
        CHECK_EQ(r.next(), e);
    }
}

TEST(rng_unit_in_unit_interval) {
    SplitMix64 r(42);
    for (int i = 0; i < 1000; ++i) {
        const float v = r.next_unit();
        CHECK(v >= 0.0f);
        CHECK(v < 1.0f);
    }
}

TEST(rng_reproducible_after_reseed) {
    SplitMix64 r(777);
    const std::uint64_t first = r.next();
    r.seed(777);
    CHECK_EQ(r.next(), first);
}

TEST(snapshot_rollback_replay_same_history) {
    // Spec: snapshot -> 10000 events -> rollback -> replay produces the same
    // history as the original run.
    FieldMap world(64, 64);
    RegionScheduler sched(world.width(), world.height(), 32);
    sched.set_idle_ticks_before_sleep(1 << 30);

    constexpr std::uint32_t STRIKES = 10000;
    SplitMix64 rng(20260731);

    auto make_lightning_queue = []() {
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
        return q;
    };

    // Original run.
    EventQueue q1 = make_lightning_queue();
    WorldSnapshot pristine = WorldSnapshot::capture(world);
    for (std::uint32_t i = 0; i < STRIKES; ++i) {
        q1.push(EventType::Lightning, rng.next_u32(64), rng.next_u32(64), 1.0f);
    }
    q1.process_all(world);
    sched.tick(world);
    const std::vector<Event> original_history = q1.history();
    WorldSnapshot original_final = WorldSnapshot::capture(world);
    CHECK_EQ(original_history.size(), 40000u);

    // Rollback to pristine, then replay the exact recorded event sequence.
    pristine.restore(world);
    CHECK(world.bitwise_equal(FieldMap(64, 64)));

    EventQueue q2 = make_lightning_queue();
    for (const Event& e : original_history) {
        // Replay only the ORIGINAL (root) events; cascades re-generate.
        if (e.type == EventType::Lightning) {
            q2.push(e.type, e.x, e.y, e.payload);
        }
    }
    q2.process_all(world);
    sched.tick(world);

    // Replay reproduces the same final world bit-exactly.
    CHECK(original_final.matches(world));

    // And the replayed history is identical to the original.
    const std::vector<Event>& replay_history = q2.history();
    CHECK_EQ(replay_history.size(), original_history.size());
    for (std::size_t i = 0; i < original_history.size(); ++i) {
        CHECK(replay_history[i].type == original_history[i].type);
        CHECK(replay_history[i].x == original_history[i].x);
        CHECK(replay_history[i].y == original_history[i].y);
        CHECK(replay_history[i].payload == original_history[i].payload);
        CHECK(replay_history[i].seq == original_history[i].seq);
    }
}

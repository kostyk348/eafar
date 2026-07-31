#include "test_runner.hpp"
#include "eafar/automaton.hpp"
#include "eafar/region_scheduler.hpp"

#include <cstdint>

using namespace eafar;

// World: 64x64, region size 32 -> 2x2 regions. Helper to wake all.
static RegionScheduler make_sched(FieldMap& world) {
    RegionScheduler s(world.width(), world.height(), 32);
    s.set_idle_ticks_before_sleep(1 << 30); // never sleep during test
    return s;
}

TEST(automaton_period_controls_cadence) {
    FieldMap world(64, 64);
    RegionScheduler s = make_sched(world);
    s.wake_region(0, 0);

    AutomatonManager m;
    std::uint64_t a = 0, b = 0, c = 0;
    m.add("fast", 1, [&](std::uint32_t, std::uint32_t, FieldMap&) { ++a; });
    m.add("mid", 2, [&](std::uint32_t, std::uint32_t, FieldMap&) { ++b; });
    m.add("slow", 3, [&](std::uint32_t, std::uint32_t, FieldMap&) { ++c; });

    for (int t = 0; t < 12; ++t) {
        m.advance(world, s);
    }

    CHECK_EQ(a, 12u); // every tick
    CHECK_EQ(b, 6u);  // every 2nd
    CHECK_EQ(c, 4u);  // every 3rd
    CHECK_EQ(m.sim_tick(), 12u);
}

TEST(spec_frequencies_60hz_10hz_01hz_1perhour) {
    // Base tick = 1/60 s.
    // Fire 60Hz -> period 1, Water 10Hz -> 6, Plants 0.1Hz -> 600,
    // Evolution 1/hour -> 3600 s = 216000 ticks.
    FieldMap world(64, 64);
    RegionScheduler s = make_sched(world);
    s.wake_region(0, 0);

    AutomatonManager m;
    m.add("Fire", 1, [&](std::uint32_t, std::uint32_t, FieldMap&) {});
    m.add("Water", 6, [&](std::uint32_t, std::uint32_t, FieldMap&) {});
    m.add("Plants", 600, [&](std::uint32_t, std::uint32_t, FieldMap&) {});
    m.add("Evolution", 216000, [&](std::uint32_t, std::uint32_t, FieldMap&) {});

    constexpr std::uint64_t TICKS = 216000; // one sim-hour
    for (std::uint64_t t = 0; t < TICKS; ++t) {
        m.advance(world, s);
    }

    const auto& as = m.automata();
    CHECK_EQ(as[0].name, "Fire");
    CHECK_EQ(as[0].executed, 216000u);   // 60 Hz * 3600 s
    CHECK_EQ(as[1].executed, 36000u);    // 10 Hz * 3600 s
    CHECK_EQ(as[2].executed, 360u);      // 0.1 Hz * 3600 s
    CHECK_EQ(as[3].executed, 1u);        // 1 / hour
}

TEST(automaton_runs_per_active_region) {
    FieldMap world(64, 64);
    RegionScheduler s = make_sched(world);
    s.wake_region(0, 0);
    s.wake_region(1, 1); // two active regions

    AutomatonManager m;
    int visits = 0;
    m.add("fire", 1, [&](std::uint32_t, std::uint32_t, FieldMap&) { ++visits; });

    m.advance(world, s);

    // One step per active region per due tick.
    CHECK_EQ(visits, 2);
    CHECK_EQ(m.at(0).executed, 1u);
}

TEST(automaton_writes_fields_not_other_automata) {
    FieldMap world(64, 64);
    RegionScheduler s = make_sched(world);
    s.wake_region(0, 0);

    AutomatonManager m;
    // Fire raises Temperature; Water raises Humidity. No cross-references.
    m.add("Fire", 1, [](std::uint32_t, std::uint32_t, FieldMap& w) {
        w.set(Field::Temperature, 0, 0, w.get(Field::Temperature, 0, 0) + 0.01f);
    });
    m.add("Water", 1, [](std::uint32_t, std::uint32_t, FieldMap& w) {
        w.set(Field::Humidity, 0, 0, w.get(Field::Humidity, 0, 0) + 0.02f);
    });

    for (int t = 0; t < 10; ++t) {
        m.advance(world, s);
    }

    // Accumulated additions drift from the decimal literal; compare with eps.
    CHECK_NEAR(world.get(Field::Temperature, 0, 0), 0.10f, 1e-6f);
    CHECK_NEAR(world.get(Field::Humidity, 0, 0), 0.20f, 1e-6f);
    CHECK_EQ(world.get(Field::Energy, 0, 0), 0.0f); // untouched
}

TEST(automaton_never_runs_in_sleeping_regions) {
    FieldMap world(64, 64);
    RegionScheduler s(64, 64, 32);
    s.set_idle_ticks_before_sleep(1); // sleep after one tick
    s.wake_region(0, 0);

    AutomatonManager m;
    int visits = 0;
    m.add("fire", 1, [&](std::uint32_t, std::uint32_t, FieldMap&) { ++visits; });

    // Runtime loop: cadence (advance) + region aging (tick).
    m.advance(world, s);
    s.tick(world); // ages region 0,0 -> sleeps
    CHECK_EQ(visits, 1);

    m.advance(world, s);
    s.tick(world);
    CHECK_EQ(visits, 1); // no active regions -> no visits
}

TEST(automaton_deterministic_across_runs) {
    // Same config, same tick count -> identical counters and world state.
    auto run_once = [](std::uint64_t& fire_count, std::uint64_t& water_count,
                       float& temp) {
        FieldMap world(64, 64);
        RegionScheduler s = make_sched(world);
        s.wake_region(1, 1);

        AutomatonManager m;
        m.add("Fire", 1, [&](std::uint32_t, std::uint32_t, FieldMap& w) {
            w.set(Field::Temperature, 0, 0, w.get(Field::Temperature, 0, 0) + 0.5f);
        });
        m.add("Water", 7, [&](std::uint32_t, std::uint32_t, FieldMap& w) {
            w.set(Field::Humidity, 0, 0, w.get(Field::Humidity, 0, 0) + 0.1f);
        });

        for (int t = 0; t < 100; ++t) {
            m.advance(world, s);
        }

        fire_count = m.at(0).executed;
        water_count = m.at(1).executed;
        temp = world.get(Field::Temperature, 0, 0);
    };

    std::uint64_t f1 = 0, f2 = 0, w1 = 0, w2 = 0;
    float t1 = 0, t2 = 0;
    run_once(f1, w1, t1);
    run_once(f2, w2, t2);

    CHECK_EQ(f1, f2);
    CHECK_EQ(w1, w2);
    CHECK_EQ(t1, t2);
}

TEST(zero_period_throws) {
    AutomatonManager m;
    bool threw = false;
    try {
        m.add("bad", 0, [](std::uint32_t, std::uint32_t, FieldMap&) {});
    } catch (...) {
        threw = true;
    }
    CHECK(threw);
    CHECK_EQ(m.count(), 0u);
}

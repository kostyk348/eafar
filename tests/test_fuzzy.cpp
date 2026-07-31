#include "test_runner.hpp"
#include "eafar/fuzzy.hpp"
#include "eafar/automaton.hpp"
#include "eafar/event_queue.hpp"
#include "eafar/region_scheduler.hpp"

#include <cstdint>

using namespace eafar;

TEST(step_membership) {
    auto s = mf::step(0.8f);
    CHECK_EQ(s(0.0f), 0.0f);
    CHECK_EQ(s(0.79f), 0.0f);
    CHECK_EQ(s(0.8f), 1.0f);
    CHECK_EQ(s(1.0f), 1.0f);
}

TEST(ramp_membership) {
    auto r = mf::ramp(0.2f, 0.8f);
    CHECK_EQ(r(0.1f), 0.0f);
    CHECK_EQ(r(0.5f), 0.5f);
    CHECK_EQ(r(0.8f), 1.0f);
    CHECK_EQ(r(1.0f), 1.0f);
}

TEST(triangle_membership) {
    auto t = mf::triangle(0.5f, 0.2f);
    CHECK_EQ(t(0.5f), 1.0f);
    // At the exact edge |d| == width float rounding gives ~1e-8, not exactly 0.
    CHECK_NEAR(t(0.3f), 0.0f, 1e-6f);
    CHECK_NEAR(t(0.4f), 0.5f, 1e-6f);
    CHECK_NEAR(t(0.6f), 0.5f, 1e-6f);
    CHECK_NEAR(t(0.0f), 0.0f, 1e-6f);
}

TEST(bell_membership) {
    auto b = mf::bell(0.5f, 0.1f);
    CHECK_NEAR(b(0.5f), 1.0f, 1e-6f);
    CHECK_NEAR(b(0.6f), 0.5f, 1e-6f);
    CHECK_NEAR(b(0.4f), 0.5f, 1e-6f);
}

TEST(registry_define_apply) {
    FuzzyRegistry reg;
    reg.define("ignition", mf::step(0.8f));
    CHECK(reg.has("ignition"));
    CHECK_EQ(reg.count(), 1u);

    CHECK_EQ(reg.apply("ignition", 0.5f), 0.0f);
    CHECK_EQ(reg.apply("ignition", 0.9f), 1.0f);
}

TEST(registry_unknown_name_throws) {
    FuzzyRegistry reg;
    reg.define("ignition", mf::step(0.8f));
    CHECK_THROWS(reg.apply("nope", 0.5f));
}

TEST(registry_define_swaps_behavior) {
    // The KEY S5 property: automaton references membership by NAME;
    // swapping the fn changes behavior without editing the automaton.
    FuzzyRegistry reg;
    reg.define("hot", mf::step(0.9f)); // sharp threshold at 0.9

    // Automaton: if "hot" membership of Temperature >= 0.5 -> set Energy 1.
    auto ignition_step = [&reg](FieldMap& w) {
        const float hot = reg.apply("hot", w.get(Field::Temperature, 0, 0));
        if (hot >= 0.5f) {
            w.set(Field::Energy, 0, 0, 1.0f);
        }
    };

    FieldMap world(4, 4);

    // Case 1: step(0.9) — 0.85 is not hot.
    world.set(Field::Temperature, 0, 0, 0.85f);
    ignition_step(world);
    CHECK_EQ(world.get(Field::Energy, 0, 0), 0.0f);

    // Swap the membership function. SAME automaton code.
    reg.define("hot", mf::ramp(0.5f, 0.8f));
    world.set(Field::Energy, 0, 0, 0.0f);
    ignition_step(world);
    // 0.85 is above hi(0.8) -> membership 1 -> ignites.
    CHECK_EQ(world.get(Field::Energy, 0, 0), 1.0f);
}

TEST(registry_swaps_without_editing_automaton_in_pipeline) {
    // Integration: a Fire automaton reads "flammable" membership; the
    // registry is swapped mid-run and the same automaton reacts differently.
    FuzzyRegistry reg;
    reg.define("flammable", mf::step(0.7f)); // value >= 0.7 -> flammable

    FieldMap world(64, 64);
    RegionScheduler s(world.width(), world.height(), 32);
    s.set_idle_ticks_before_sleep(1 << 30);
    s.wake_region(0, 0);

    AutomatonManager m;
    m.add("Fire", 1, [&reg](std::uint32_t, std::uint32_t, FieldMap& w) {
        const float f = reg.apply("flammable", w.get(Field::Humidity, 0, 0));
        // Flammable -> fire consumes Energy.
        if (f >= 0.5f) {
            w.set(Field::Energy, 0, 0, 0.0f);
        }
    });

    // Wet world (humidity 0.9): flammable under step(0.7) -> energy consumed.
    world.set(Field::Humidity, 0, 0, 0.9f);
    world.set(Field::Energy, 0, 0, 1.0f);
    m.advance(world, s);
    CHECK_EQ(world.get(Field::Energy, 0, 0), 0.0f);

    // Dry world (humidity 0.2): NOT flammable under step(0.7) -> untouched.
    world.set(Field::Humidity, 0, 0, 0.2f);
    world.set(Field::Energy, 0, 0, 1.0f);
    m.advance(world, s);
    CHECK_EQ(world.get(Field::Energy, 0, 0), 1.0f);

    // Swap to "dry = flammable" semantics: SAME automaton now consumes.
    reg.define("flammable", [](float v) { return v < 0.8f ? 1.0f : 0.0f; });
    m.advance(world, s);
    CHECK_EQ(world.get(Field::Energy, 0, 0), 0.0f);
}

TEST(clamp01_bounds) {
    CHECK_EQ(clamp01(-1.0f), 0.0f);
    CHECK_EQ(clamp01(0.5f), 0.5f);
    CHECK_EQ(clamp01(1.5f), 1.0f);
}

TEST(null_membership_rejected) {
    FuzzyRegistry reg;
    CHECK_THROWS(reg.define("bad", nullptr));
}

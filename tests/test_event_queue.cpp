#include "test_runner.hpp"
#include "eafar/event_queue.hpp"
#include "eafar/field_map.hpp"

#include <cstdint>

using namespace eafar;

// ---------------------------------------------------------------------------
// The spec cascade: Lightning -> Fire -> Smoke -> Plants.
// Each handler writes a field (S1 coupling) and pushes the next event.
// No handler references any other handler — only fields and the queue.
// ---------------------------------------------------------------------------

// Moderate values so the arithmetic below stays in [0,1] for a single strike.
static void subscribe_cascade(EventQueue& q) {
    q.subscribe(EventType::Lightning, [](const Event& e, FieldMap& w, EventQueue& q) {
        // Lightning deposits heat at its cell.
        float t = w.get(Field::Temperature, e.x, e.y);
        w.set(Field::Temperature, e.x, e.y, t + 0.50f);
        // ... and ignites the cell.
        q.push(EventType::Fire, e.x, e.y, 0.90f);
    });
    q.subscribe(EventType::Fire, [](const Event& e, FieldMap& w, EventQueue& q) {
        // Fire burns fuel (Energy) and keeps raising Temperature.
        float t = w.get(Field::Temperature, e.x, e.y);
        w.set(Field::Temperature, e.x, e.y, t + 0.20f);
        w.set(Field::Energy, e.x, e.y, 0.05f); // fuel mostly consumed
        // Smoke carries the burn upward/outward.
        q.push(EventType::Smoke, e.x, e.y, 0.70f);
    });
    q.subscribe(EventType::Smoke, [](const Event& e, FieldMap& w, EventQueue& q) {
        // Smoke chokes Population at the cell.
        float p = w.get(Field::Population, e.x, e.y);
        w.set(Field::Population, e.x, e.y, p - 0.40f);
        // After the smoke, plants regrow.
        q.push(EventType::Plants, e.x, e.y, 0.30f);
    });
    q.subscribe(EventType::Plants, [](const Event& e, FieldMap& w, EventQueue&) {
        // Plants recover Population and restore Energy.
        float p = w.get(Field::Population, e.x, e.y);
        w.set(Field::Population, e.x, e.y, p + 0.10f);
        w.set(Field::Energy, e.x, e.y, 0.40f);
    });
}

TEST(event_queue_cascade_lightning_to_plants) {
    FieldMap world(16, 16);
    EventQueue q;
    subscribe_cascade(q);

    q.push(EventType::Lightning, 5, 5, 1.0f);
    CHECK(!q.empty());

    q.process_all(world);

    // Queue fully drained, no global loop needed.
    CHECK(q.empty());
    CHECK_EQ(q.pending_count(), 0u);

    // The full chain fired, in FIFO order, exactly once per type.
    const auto& h = q.history();
    CHECK_EQ(h.size(), 4u);
    CHECK(h[0].type == EventType::Lightning);
    CHECK(h[1].type == EventType::Fire);
    CHECK(h[2].type == EventType::Smoke);
    CHECK(h[3].type == EventType::Plants);

    // Sequential numbers are monotonic (replay groundwork).
    CHECK(h[0].seq < h[1].seq);
    CHECK(h[1].seq < h[2].seq);
    CHECK(h[2].seq < h[3].seq);

    // Fields reflect the cascade at the strike cell.
    // Temperature: 0.50 + 0.20 = 0.70
    CHECK_EQ(world.get(Field::Temperature, 5, 5), 0.70f);
    // Population: -0.40 + 0.10 = -0.30 (can go negative; fuzzy layer clamps later)
    CHECK_EQ(world.get(Field::Population, 5, 5), -0.30f);
    // Energy: 0.05 -> 0.40 (plants restored it)
    CHECK_EQ(world.get(Field::Energy, 5, 5), 0.40f);
}

TEST(event_cascade_is_local_to_cell) {
    FieldMap world(16, 16);
    EventQueue q;
    subscribe_cascade(q);

    // Two independent strikes in different cells.
    q.push(EventType::Lightning, 2, 2, 1.0f);
    q.push(EventType::Lightning, 13, 9, 1.0f);
    q.process_all(world);

    // Both cells show the full cascade result...
    CHECK_EQ(world.get(Field::Temperature, 2, 2), 0.70f);
    CHECK_EQ(world.get(Field::Temperature, 13, 9), 0.70f);
    // ...and no other cell was touched (no global update).
    CHECK_EQ(world.get(Field::Temperature, 0, 0), 0.0f);
    CHECK_EQ(world.get(Field::Temperature, 5, 5), 0.0f);
    CHECK_EQ(world.get(Field::Energy, 8, 8), 0.0f);

    // Two interleaved chains processed in FIFO order.
    CHECK_EQ(q.history().size(), 8u);
}

TEST(queue_without_handlers_drains_quietly) {
    FieldMap world(8, 8);
    EventQueue q;
    q.push(EventType::Fire, 1, 1, 0.5f);
    q.push(EventType::Smoke, 1, 1, 0.5f);

    q.process_all(world);

    CHECK(q.empty());
    CHECK_EQ(q.processed_count(), 2u);
    CHECK_EQ(q.history().size(), 2u);
    // No handler -> world untouched.
    CHECK_EQ(world.get(Field::Temperature, 1, 1), 0.0f);
}

TEST(multiple_handlers_per_type_all_run) {
    FieldMap world(8, 8);
    EventQueue q;
    int calls = 0;
    q.subscribe(EventType::Fire, [&](const Event&, FieldMap&, EventQueue&) { ++calls; });
    q.subscribe(EventType::Fire, [&](const Event&, FieldMap&, EventQueue&) { ++calls; });
    q.subscribe(EventType::Fire, [&](const Event&, FieldMap&, EventQueue&) { ++calls; });

    q.push(EventType::Fire, 0, 0, 1.0f);
    q.process_all(world);

    CHECK_EQ(calls, 3);
}

TEST(seq_numbers_are_deterministic) {
    FieldMap world(4, 4);
    EventQueue a, b;

    a.push(EventType::Lightning, 1, 1, 1.0f);
    a.push(EventType::Fire, 2, 2, 1.0f);
    b.push(EventType::Lightning, 1, 1, 1.0f);
    b.push(EventType::Fire, 2, 2, 1.0f);

    a.process_all(world);
    b.process_all(world);

    // Identical push sequence -> identical history (sequence numbers included).
    CHECK_EQ(a.history().size(), b.history().size());
    for (std::size_t i = 0; i < a.history().size(); ++i) {
        const Event& ea = a.history()[i];
        const Event& eb = b.history()[i];
        CHECK(ea.type == eb.type);
        CHECK(ea.x == eb.x && ea.y == eb.y);
        CHECK(ea.payload == eb.payload);
        CHECK(ea.seq == eb.seq);
    }
}

TEST(invalid_event_type_throws) {
    EventQueue q;
    CHECK_THROWS(q.push(EventType::_count, 0, 0, 1.0f));
    CHECK_THROWS(q.subscribe(EventType::_count, [](const Event&, FieldMap&, EventQueue&) {}));
}

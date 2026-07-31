#include "test_runner.hpp"
#include "eafar/field_map.hpp"

#include <cstring>

using namespace eafar;

TEST(default_constructed_map_is_empty) {
    FieldMap m;
    CHECK_EQ(m.width(), 0u);
    CHECK_EQ(m.height(), 0u);
    CHECK_EQ(m.cell_count(), 0u);
}

TEST(constructor_sets_dimensions) {
    FieldMap m(32, 16);
    CHECK_EQ(m.width(), 32u);
    CHECK_EQ(m.height(), 16u);
    CHECK_EQ(m.cell_count(), 512u);
}

TEST(new_map_is_zeroed) {
    FieldMap m(8, 8);
    for (Field f : kAllFields) {
        for (std::uint32_t y = 0; y < 8; ++y) {
            for (std::uint32_t x = 0; x < 8; ++x) {
                CHECK_EQ(m.get(f, x, y), 0.0f);
            }
        }
    }
}

TEST(set_get_roundtrip_all_fields) {
    FieldMap m(4, 4);
    // Distinct value per field, per coordinate.
    for (std::size_t fi = 0; fi < kFieldCount; ++fi) {
        Field f = kAllFields[fi];
        float v = 0.5f + static_cast<float>(fi) * 0.1f;
        m.set(f, 1, 2, v);
        CHECK_EQ(m.get(f, 1, 2), v);
        // untouched cells stay zero
        CHECK_EQ(m.get(f, 0, 0), 0.0f);
    }
}

TEST(soa_layout_single_array_per_field) {
    FieldMap m(10, 10);
    // Each field's storage must be one contiguous array of cell_count floats.
    for (Field f : kAllFields) {
        const float* p = m.data(f);
        CHECK(p != nullptr);
        // write through the raw pointer, read back via get()
        const std::size_t idx = 5 * 10 + 7; // (7, 5)
        const_cast<float*>(p)[idx] = 0.75f;
        CHECK_EQ(m.get(f, 7, 5), 0.75f);
    }
}

TEST(soa_fields_do_not_alias) {
    FieldMap m(8, 8);
    m.set(Field::Temperature, 0, 0, 1.0f);
    m.set(Field::Humidity, 0, 0, 2.0f);
    // Fields must be independent arrays (no per-object struct aliasing).
    CHECK_EQ(m.get(Field::Temperature, 0, 0), 1.0f);
    CHECK_EQ(m.get(Field::Humidity, 0, 0), 2.0f);
}

TEST(fill_sets_entire_field) {
    FieldMap m(16, 16);
    m.fill(Field::Radiation, 0.42f);
    for (std::uint32_t y = 0; y < 16; ++y) {
        for (std::uint32_t x = 0; x < 16; ++x) {
            CHECK_EQ(m.get(Field::Radiation, x, y), 0.42f);
        }
    }
    CHECK_EQ(m.get(Field::Temperature, 15, 15), 0.0f); // others untouched
}

TEST(out_of_bounds_throws) {
    FieldMap m(8, 8);
    CHECK_THROWS(m.get(Field::Temperature, 8, 0));
    CHECK_THROWS(m.get(Field::Temperature, 0, 8));
    CHECK_THROWS(m.set(Field::Temperature, 9, 9, 1.0f));
}

TEST(bitwise_equal_detects_change) {
    FieldMap a(16, 16);
    FieldMap b(16, 16);
    CHECK(a.bitwise_equal(b));

    b.set(Field::Energy, 3, 4, 1e-9f); // tiny but non-zero delta
    CHECK(!a.bitwise_equal(b));

    b.set(Field::Energy, 3, 4, 0.0f);
    CHECK(a.bitwise_equal(b)); // restoring the exact bits restores equality
}

TEST(different_dimensions_not_equal) {
    FieldMap a(16, 16);
    FieldMap b(8, 32);
    CHECK(!a.bitwise_equal(b));
}

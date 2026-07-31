#pragma once
// eafar/field_map.hpp — S1: Spatial State Storage.
//
// The world is stored as SoA (Struct-of-Arrays) fields: one contiguous
// float array per semantic field. There are NO per-cell objects — a cell
// is just an index shared by all fields. This is what makes snapshots
// (memcpy of N arrays), parallelism and field-level diffing cheap.

#include "eafar/field.hpp"

#include <array>
#include <cstdint>
#include <cstddef>
#include <vector>

namespace eafar {

class FieldMap {
public:
    FieldMap() = default;
    FieldMap(std::uint32_t width, std::uint32_t height);

    std::uint32_t width() const noexcept { return width_; }
    std::uint32_t height() const noexcept { return height_; }
    std::size_t cell_count() const noexcept { return static_cast<std::size_t>(width_) * height_; }

    bool in_bounds(std::uint32_t x, std::uint32_t y) const noexcept {
        return x < width_ && y < height_;
    }
    std::size_t index(std::uint32_t x, std::uint32_t y) const noexcept {
        return static_cast<std::size_t>(y) * width_ + x;
    }

    // Checked access (throws std::out_of_range on OOB).
    float get(Field f, std::uint32_t x, std::uint32_t y) const;
    void set(Field f, std::uint32_t x, std::uint32_t y, float value);

    // Raw SoA access for bulk / diff / snapshot paths.
    float* data(Field f) noexcept;
    const float* data(Field f) const noexcept;

    void fill(Field f, float value);
    void clear() noexcept; // zero all fields

    // Deterministic equality: bit-exact compare of every field array.
    bool bitwise_equal(const FieldMap& other) const noexcept;

private:
    std::uint32_t width_ = 0;
    std::uint32_t height_ = 0;
    std::array<std::vector<float>, kFieldCount> fields_;
};

} // namespace eafar

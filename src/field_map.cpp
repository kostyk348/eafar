#include "eafar/field_map.hpp"

#include <cstring>
#include <stdexcept>

namespace eafar {

FieldMap::FieldMap(std::uint32_t width, std::uint32_t height)
    : width_(width), height_(height) {
    const std::size_t n = cell_count();
    for (auto& arr : fields_) {
        arr.assign(n, 0.0f);
    }
}

float FieldMap::get(Field f, std::uint32_t x, std::uint32_t y) const {
    if (!in_bounds(x, y)) {
        throw std::out_of_range("eafar: FieldMap::get out of bounds");
    }
    const std::size_t idx = index(x, y);
    return fields_[static_cast<std::size_t>(f)][idx];
}

void FieldMap::set(Field f, std::uint32_t x, std::uint32_t y, float value) {
    if (!in_bounds(x, y)) {
        throw std::out_of_range("eafar: FieldMap::set out of bounds");
    }
    const std::size_t idx = index(x, y);
    fields_[static_cast<std::size_t>(f)][idx] = value;
}

float* FieldMap::data(Field f) noexcept {
    return fields_[static_cast<std::size_t>(f)].data();
}

const float* FieldMap::data(Field f) const noexcept {
    return fields_[static_cast<std::size_t>(f)].data();
}

void FieldMap::fill(Field f, float value) {
    auto& arr = fields_[static_cast<std::size_t>(f)];
    std::fill(arr.begin(), arr.end(), value);
}

void FieldMap::clear() noexcept {
    for (auto& arr : fields_) {
        std::fill(arr.begin(), arr.end(), 0.0f);
    }
}

bool FieldMap::bitwise_equal(const FieldMap& other) const noexcept {
    if (width_ != other.width_ || height_ != other.height_) {
        return false;
    }
    const std::size_t bytes = cell_count() * sizeof(float);
    for (std::size_t i = 0; i < kFieldCount; ++i) {
        if (std::memcmp(fields_[i].data(), other.fields_[i].data(), bytes) != 0) {
            return false;
        }
    }
    return true;
}

} // namespace eafar

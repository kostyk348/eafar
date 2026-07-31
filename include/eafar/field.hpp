#pragma once
// eafar/field.hpp — semantic field types.
// "Everything is a field": the world is a set of independent SoA fields.
// Automata read/write fields; they never reference each other directly.

#include <array>
#include <cstdint>
#include <string_view>

namespace eafar {

enum class Field : std::uint8_t {
    Temperature = 0,
    Humidity,
    Energy,
    Radiation,
    Population,
    Corruption,
    _count
};

inline constexpr std::size_t kFieldCount = static_cast<std::size_t>(Field::_count);

inline constexpr std::array<Field, kFieldCount> kAllFields = {
    Field::Temperature, Field::Humidity, Field::Energy,
    Field::Radiation, Field::Population, Field::Corruption
};

constexpr std::string_view field_name(Field f) noexcept {
    using enum Field;
    switch (f) {
        case Temperature: return "Temperature";
        case Humidity:    return "Humidity";
        case Energy:      return "Energy";
        case Radiation:   return "Radiation";
        case Population:  return "Population";
        case Corruption:  return "Corruption";
        case _count:      break;
    }
    return "?";
}

} // namespace eafar

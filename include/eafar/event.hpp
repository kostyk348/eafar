#pragma once
// eafar/event.hpp — event types and event struct.
// Events are the ONLY way the world changes (no update_everything).
// payload is a single float in [0..1] (fuzzy-world convention).

#include <cstdint>
#include <string_view>

namespace eafar {

enum class EventType : std::uint8_t {
    Lightning = 0, // spec cascade: Lightning -> Fire -> Smoke -> Plants
    Fire,
    Smoke,
    Plants,
    // Generic slots for user-defined types beyond the builtin demo set.
    Custom0,
    Custom1,
    Custom2,
    Custom3,
    _count
};

inline constexpr std::size_t kEventTypeCount = static_cast<std::size_t>(EventType::_count);

constexpr std::string_view event_type_name(EventType t) noexcept {
    using enum EventType;
    switch (t) {
        case Lightning: return "Lightning";
        case Fire:      return "Fire";
        case Smoke:     return "Smoke";
        case Plants:    return "Plants";
        case Custom0:   return "Custom0";
        case Custom1:   return "Custom1";
        case Custom2:   return "Custom2";
        case Custom3:   return "Custom3";
        case _count:    break;
    }
    return "?";
}

struct Event {
    EventType type;
    std::uint32_t x;
    std::uint32_t y;
    float payload;         // magnitude in [0..1]
    std::uint64_t seq;     // monotonic sequence number (determinism / replay)
};

} // namespace eafar

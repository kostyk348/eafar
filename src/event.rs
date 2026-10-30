//! Event types and the event struct.
//!
//! C++ lineage: `include/eafar/event.hpp`.
//!
//! Events are the ONLY way the world changes (there is no
//! `update_everything`). `payload` is a single float in `[0..1]`
//! (fuzzy-world convention).

/// Kind of event (C++ `enum class EventType : std::uint8_t`).
///
/// The C++ `_count` sentinel does not exist in Rust: the enum type system
/// makes invalid event types unrepresentable, which is exactly the check the
/// C++ `push`/`subscribe` performed at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum EventType {
    /// Spec cascade: `Lightning -> Fire -> Smoke -> Plants`.
    Lightning = 0,
    Fire,
    Smoke,
    Plants,
    /// Generic slots for user-defined types beyond the builtin demo set.
    Custom0,
    Custom1,
    Custom2,
    Custom3,
}

/// Number of event types (C++ `kEventTypeCount`).
pub const EVENT_TYPE_COUNT: usize = 8;

impl EventType {
    /// All event types in declaration order.
    pub const ALL: [EventType; EVENT_TYPE_COUNT] = [
        EventType::Lightning,
        EventType::Fire,
        EventType::Smoke,
        EventType::Plants,
        EventType::Custom0,
        EventType::Custom1,
        EventType::Custom2,
        EventType::Custom3,
    ];

    /// Human-readable name (C++ `event_type_name`).
    pub const fn name(self) -> &'static str {
        match self {
            EventType::Lightning => "Lightning",
            EventType::Fire => "Fire",
            EventType::Smoke => "Smoke",
            EventType::Plants => "Plants",
            EventType::Custom0 => "Custom0",
            EventType::Custom1 => "Custom1",
            EventType::Custom2 => "Custom2",
            EventType::Custom3 => "Custom3",
        }
    }
}

/// Free-standing event type name, mirroring the C++ `event_type_name` helper.
pub const fn event_type_name(t: EventType) -> &'static str {
    t.name()
}

/// A single event flowing through the queue.
///
/// C++ lineage: `struct Event`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    /// Event kind.
    pub type_: EventType,
    /// World x coordinate the event targets.
    pub x: u32,
    /// World y coordinate the event targets.
    pub y: u32,
    /// Magnitude in `[0..1]` (fuzzy-world convention).
    pub payload: f32,
    /// Monotonic sequence number (determinism / replay groundwork).
    pub seq: u64,
}

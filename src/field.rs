//! Semantic field types.
//!
//! C++ lineage: `include/eafar/field.hpp`.
//!
//! "Everything is a field": the world is a set of independent SoA fields.
//! Automata read/write fields; they never reference each other directly.

/// A semantic field of the world (C++ `enum class Field : std::uint8_t`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Field {
    Temperature = 0,
    Humidity,
    Energy,
    Radiation,
    Population,
    Corruption,
}

/// Number of semantic fields (C++ `kFieldCount`).
pub const FIELD_COUNT: usize = 6;

impl Field {
    /// All fields in declaration order (C++ `kAllFields`).
    pub const ALL: [Field; FIELD_COUNT] = [
        Field::Temperature,
        Field::Humidity,
        Field::Energy,
        Field::Radiation,
        Field::Population,
        Field::Corruption,
    ];

    /// Human-readable name (C++ `field_name`).
    pub const fn name(self) -> &'static str {
        match self {
            Field::Temperature => "Temperature",
            Field::Humidity => "Humidity",
            Field::Energy => "Energy",
            Field::Radiation => "Radiation",
            Field::Population => "Population",
            Field::Corruption => "Corruption",
        }
    }
}

/// Free-standing field name, mirroring the C++ `field_name(Field)` helper.
pub const fn field_name(f: Field) -> &'static str {
    f.name()
}

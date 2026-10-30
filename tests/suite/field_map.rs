//! Rust port of `tests/test_field_map.cpp` (10 tests).

use eafar::prelude::*;

#[test]
fn default_constructed_map_is_empty() {
    let m = FieldMap::default();
    check_eq!(m.width(), 0);
    check_eq!(m.height(), 0);
    check_eq!(m.cell_count(), 0);
}

#[test]
fn constructor_sets_dimensions() {
    let m = FieldMap::new(32, 16);
    check_eq!(m.width(), 32);
    check_eq!(m.height(), 16);
    check_eq!(m.cell_count(), 512);
}

#[test]
fn new_map_is_zeroed() {
    let m = FieldMap::new(8, 8);
    for f in Field::ALL {
        for y in 0..8u32 {
            for x in 0..8u32 {
                check_eq!(m.get(f, x, y).unwrap(), 0.0);
            }
        }
    }
}

#[test]
fn set_get_roundtrip_all_fields() {
    let mut m = FieldMap::new(4, 4);
    // Distinct value per field, per coordinate.
    for (fi, &f) in Field::ALL.iter().enumerate() {
        let v = 0.5 + fi as f32 * 0.1;
        m.set(f, 1, 2, v).unwrap();
        check_eq!(m.get(f, 1, 2).unwrap(), v);
        // untouched cells stay zero
        check_eq!(m.get(f, 0, 0).unwrap(), 0.0);
    }
}

#[test]
fn soa_layout_single_array_per_field() {
    let mut m = FieldMap::new(10, 10);
    // Each field's storage must be one contiguous array of cell_count floats.
    for f in Field::ALL {
        // write through the raw slice, read back via get()
        let idx = 5 * 10 + 7; // (7, 5)
        m.data_mut(f)[idx] = 0.75;
        check_eq!(m.get(f, 7, 5).unwrap(), 0.75);
    }
}

#[test]
fn soa_fields_do_not_alias() {
    let mut m = FieldMap::new(8, 8);
    m.set(Field::Temperature, 0, 0, 1.0).unwrap();
    m.set(Field::Humidity, 0, 0, 2.0).unwrap();
    // Fields must be independent arrays (no per-object struct aliasing).
    check_eq!(m.get(Field::Temperature, 0, 0).unwrap(), 1.0);
    check_eq!(m.get(Field::Humidity, 0, 0).unwrap(), 2.0);
}

#[test]
fn fill_sets_entire_field() {
    let mut m = FieldMap::new(16, 16);
    m.fill(Field::Radiation, 0.42);
    for y in 0..16u32 {
        for x in 0..16u32 {
            check_eq!(m.get(Field::Radiation, x, y).unwrap(), 0.42);
        }
    }
    check_eq!(m.get(Field::Temperature, 15, 15).unwrap(), 0.0); // others untouched
}

#[test]
fn out_of_bounds_throws() {
    let mut m = FieldMap::new(8, 8);
    check_throws!(m.get(Field::Temperature, 8, 0));
    check_throws!(m.get(Field::Temperature, 0, 8));
    check_throws!(m.set(Field::Temperature, 9, 9, 1.0));
}

#[test]
fn bitwise_equal_detects_change() {
    let a = FieldMap::new(16, 16);
    let mut b = FieldMap::new(16, 16);
    check!(a.bitwise_equal(&b));

    b.set(Field::Energy, 3, 4, 1e-9).unwrap(); // tiny but non-zero delta
    check!(!a.bitwise_equal(&b));

    b.set(Field::Energy, 3, 4, 0.0).unwrap();
    check!(a.bitwise_equal(&b)); // restoring the exact bits restores equality
}

#[test]
fn different_dimensions_not_equal() {
    let a = FieldMap::new(16, 16);
    let b = FieldMap::new(8, 32);
    check!(!a.bitwise_equal(&b));
}

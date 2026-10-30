//! Rust port of `tests/test_visualizer.cpp` (6 tests).
//!
//! The C++ tests wrote `viz_*` files next to the test binary; the Rust
//! port writes to a per-process temp directory instead.

use std::path::PathBuf;

use eafar::prelude::*;

/// Unique temp output path for one test artifact.
fn out_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eafar_viz_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn read_file(path: &std::path::Path) -> Vec<u8> {
    std::fs::read(path).expect("test artifact must be readable")
}

#[test]
fn csv_heatmap_export() {
    let mut world = FieldMap::new(4, 4);
    world.set(Field::Temperature, 1, 2, 0.75).unwrap();

    let path = out_path("heat.csv");
    Visualizer::export_heatmap_csv(&world, Field::Temperature, &path).unwrap();

    let content = String::from_utf8(read_file(&path)).unwrap();

    check!(content.contains("x,y,Temperature"));
    check!(content.contains("1,2,0.75")); // the hot cell
    // 4x4 = 16 data rows + 1 header.
    let lines = content.bytes().filter(|&c| c == b'\n').count();
    check_eq!(lines, 17);
}

#[test]
fn png_heatmap_export_valid_signature() {
    let mut world = FieldMap::new(8, 8);
    world.set(Field::Radiation, 4, 4, 1.0).unwrap();

    let path = out_path("heat.png");
    Visualizer::export_heatmap_png(&world, Field::Radiation, &path, 0.0, 1.0).unwrap();

    let bytes = read_file(&path);
    check!(bytes.len() > 64); // not empty

    // PNG signature.
    let sig: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    check!(bytes[..8] == sig);

    // IHDR present with correct dimensions (8x8, 8-bit, RGB).
    check!(bytes[12] == b'I' && bytes[13] == b'H' && bytes[14] == b'D' && bytes[15] == b'R');
    check_eq!(bytes[16], 0); // width high byte
    check_eq!(bytes[20], 0); // height high byte
    check_eq!(bytes[24], 8); // bit depth
    check_eq!(bytes[25], 2); // color type RGB

    // IDAT present; IEND is the last chunk. Distance from the "IEND" marker
    // to the end of file = marker(4) + crc(4) = 8, exactly like the C++
    // `std::distance(iend, bytes.end()) == 8` check.
    let iend_pos = bytes
        .windows(4)
        .position(|w| w == b"IEND")
        .expect("IEND chunk must be present");
    check_eq!(bytes.len() - iend_pos, 8);
    // The chunk length field before IEND says 0 (empty payload).
    let len_pos = iend_pos - 4;
    check_eq!(bytes[len_pos], 0);
    check_eq!(bytes[len_pos + 1], 0);
    check_eq!(bytes[len_pos + 2], 0);
    check_eq!(bytes[len_pos + 3], 0);
}

#[test]
fn png_export_deterministic_bytes() {
    let mut world = FieldMap::new(6, 6);
    world.set(Field::Temperature, 2, 2, 0.5).unwrap();

    let p1 = out_path("det1.png");
    let p2 = out_path("det2.png");
    Visualizer::export_heatmap_png(&world, Field::Temperature, &p1, 0.0, 1.0).unwrap();
    Visualizer::export_heatmap_png(&world, Field::Temperature, &p2, 0.0, 1.0).unwrap();

    let a = read_file(&p1);
    let b = read_file(&p2);
    check!(a == b); // deterministic: same world -> identical bytes
}

#[test]
fn regions_png_overlay() {
    let mut world = FieldMap::new(64, 64);
    world.set(Field::Temperature, 40, 40, 1.0).unwrap(); // hot cell in region (1,1)

    let mut sched = RegionScheduler::new(64, 64, 32).unwrap();
    sched.set_idle_ticks_before_sleep(1 << 30);
    sched.wake_region(1, 1).unwrap();

    let path = out_path("regions.png");
    Visualizer::export_regions_png(&world, &sched, Field::Temperature, &path, 0.0, 1.0)
        .unwrap();

    let bytes = read_file(&path);
    check!(bytes.len() > 64);
    let sig: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    check!(bytes[..8] == sig);
}

#[test]
fn sleeping_cell_count() {
    let world = FieldMap::new(64, 64); // 2x2 regions of 32x32 = 4096 cells
    let mut sched = RegionScheduler::new(64, 64, 32).unwrap();
    sched.set_idle_ticks_before_sleep(1 << 30);

    check_eq!(Visualizer::sleeping_cells(&world, &sched), 4096); // all asleep

    sched.wake_region(0, 0).unwrap();
    check_eq!(Visualizer::sleeping_cells(&world, &sched), 4096 - 32 * 32); // 3072

    sched.wake_region(1, 1).unwrap();
    check_eq!(Visualizer::sleeping_cells(&world, &sched), 2048); // half awake
}

#[test]
fn write_rgba_png_direct() {
    // 1x1 red pixel PNG.
    let rgba: Vec<u8> = vec![255, 0, 0, 255];
    let path = out_path("pixel.png");
    Visualizer::write_rgba_png(1, 1, &rgba, &path).unwrap();

    let bytes = read_file(&path);
    check!(bytes.len() > 64);
    // IHDR width=1 height=1.
    check_eq!(bytes[16], 0);
    check_eq!(bytes[17], 0);
    check_eq!(bytes[18], 0);
    check_eq!(bytes[19], 1);
    check_eq!(bytes[20], 0);
    check_eq!(bytes[21], 0);
    check_eq!(bytes[22], 0);
    check_eq!(bytes[23], 1);
}

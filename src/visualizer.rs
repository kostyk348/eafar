//! S10: Visualization API.
//!
//! C++ lineage: `include/eafar/visualizer.hpp` + `src/visualizer.cpp`.
//!
//! No GUI, no OpenGL — just exports that let any external tool render:
//!   * CSV  — one heatmap per file, machine-readable
//!   * PNG  — dependency-free heatmap renderer (stored-deflate PNG writer)
//!   * overlay of active regions on top of a field
//!   * sleeping-cell statistics
//!
//! Output is deterministic: same world → same bytes.

use std::path::Path;

use crate::error::{Error, Result};
use crate::field::Field;
use crate::field_map::FieldMap;
use crate::region_scheduler::RegionScheduler;

// ---------------------------------------------------------------------------
// Dependency-free PNG writer (stored-deflate, RGB 8-bit).
// Produces valid PNG files readable by any viewer; just not compressed.
// Ported 1:1 from the C++ anonymous-namespace helpers in visualizer.cpp.
// ---------------------------------------------------------------------------

/// PNG file signature.
const PNG_SIGNATURE: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

/// CRC-32 table (IEEE 802.3, reflected, poly 0xEDB88320), built at compile
/// time — replaces the C++ lazily-initialized static table.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut n = 0usize;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
};

/// CRC-32 over a byte slice (same algorithm as the C++ `crc32`).
fn crc32(data: &[u8]) -> u32 {
    let mut c: u32 = 0xFFFF_FFFF;
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

/// Append a big-endian `u32` (C++ `append_be32`).
fn append_be32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

/// Append a PNG chunk: length + type + payload + CRC (C++ `write_chunk`).
fn write_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], payload: &[u8]) {
    append_be32(out, payload.len() as u32);
    let type_pos = out.len();
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(payload);
    let crc = crc32(&out[type_pos..]);
    append_be32(out, crc);
}

/// Adler-32 over the raw (filter-byte-prefixed) scanline stream, as the
/// zlib container requires (C++ `adler32`).
fn adler32(data: &[u8]) -> u32 {
    let mut s1: u32 = 1;
    let mut s2: u32 = 0;
    for &b in data {
        s1 = (s1 + b as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    (s2 << 16) | s1
}

/// Build a complete PNG file from RGBA pixel data (C++ `build_png`).
pub(crate) fn build_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::from(PNG_SIGNATURE);

    // IHDR: 8-bit RGB (color type 2).
    let mut ihdr = Vec::with_capacity(13);
    append_be32(&mut ihdr, w);
    append_be32(&mut ihdr, h);
    ihdr.push(8); // bit depth
    ihdr.push(2); // color type RGB
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_chunk(&mut out, b"IHDR", &ihdr);

    // Scanlines with filter byte 0 (None).
    let mut raw = Vec::with_capacity(h as usize * (w as usize * 3 + 1));
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            let i = ((y as usize) * w as usize + x as usize) * 4;
            raw.push(rgba[i]);
            raw.push(rgba[i + 1]);
            raw.push(rgba[i + 2]);
        }
    }

    // zlib stream: header 0x78 0x01 (no compression, dict off) + stored
    // deflate blocks + adler32 of raw data.
    let mut idat_payload = Vec::with_capacity(raw.len() + raw.len() / 65535 * 5 + 16);
    idat_payload.push(0x78);
    idat_payload.push(0x01);

    let mut pos = 0usize;
    let raw_size = raw.len();
    loop {
        let block = (raw_size - pos).min(65535);
        let last = pos + block == raw_size;
        idat_payload.push(if last { 0x01 } else { 0x00 }); // BFINAL|BTYPE=stored
        let len = block as u16;
        idat_payload.push((len & 0xFF) as u8);
        idat_payload.push((len >> 8) as u8);
        let nlen = !len;
        idat_payload.push((nlen & 0xFF) as u8);
        idat_payload.push((nlen >> 8) as u8);
        idat_payload.extend_from_slice(&raw[pos..pos + block]);
        pos += block;
        if pos >= raw_size {
            break;
        }
    }

    let ad = adler32(&raw);
    append_be32(&mut idat_payload, ad);
    write_chunk(&mut out, b"IDAT", &idat_payload);

    write_chunk(&mut out, b"IEND", &[]);
    out
}

/// Blue → green → yellow → red ramp for value in `[0,1]`
/// (C++ `heat_color`).
fn heat_color(t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let seg = t * 3.0;
    if seg < 1.0 {
        // blue -> cyan
        (0, (seg * 255.0) as u8, 255)
    } else if seg < 2.0 {
        // cyan -> yellow
        (
            ((seg - 1.0) * 255.0) as u8,
            255,
            ((2.0 - seg) * 255.0) as u8,
        )
    } else {
        // yellow -> red
        (255, ((3.0 - seg) * 255.0) as u8, 0)
    }
}

/// Render a field into an RGBA buffer, optionally outlining active region
/// boundaries in white (C++ `field_to_rgba`).
fn field_to_rgba(
    world: &FieldMap,
    field: Field,
    min_value: f32,
    max_value: f32,
    overlay: Option<&RegionScheduler>,
) -> Result<Vec<u8>> {
    let (w, h) = (world.width(), world.height());
    let span = if max_value > min_value {
        max_value - min_value
    } else {
        1.0
    };

    let mut rgba = vec![0u8; w as usize * h as usize * 4];
    for y in 0..h {
        for x in 0..w {
            let i = ((y as usize) * w as usize + x as usize) * 4;
            let v = world.get(field, x, y)?;
            let t = (v - min_value) / span;

            // If a region boundary passes between this cell and its east
            // or south neighbor, draw a white 1px border.
            let mut border = false;
            if let Some(overlay) = overlay {
                let active = overlay.is_active_id(overlay.region_id(x, y));
                let east = if x + 1 < w {
                    overlay.is_active_id(overlay.region_id(x + 1, y))
                } else {
                    false
                };
                let south = if y + 1 < h {
                    overlay.is_active_id(overlay.region_id(x, y + 1))
                } else {
                    false
                };
                border = (active != east) || (active != south);
            }

            if border {
                rgba[i] = 255;
                rgba[i + 1] = 255;
                rgba[i + 2] = 255;
                rgba[i + 3] = 255;
            } else {
                let (r, g, b) = heat_color(t);
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = 255;
            }
        }
    }
    Ok(rgba)
}

/// Formats a float the way the C++ CSV exporter did: `std::ostream <<`
/// with the default precision (6 significant digits, `%g` style —
/// trailing zeros stripped, scientific notation outside the fixed range).
fn format_g6(v: f32) -> String {
    if v.is_nan() {
        return if v.is_sign_negative() {
            "-nan".to_string()
        } else {
            "nan".to_string()
        };
    }
    if v.is_infinite() {
        return if v < 0.0 { "-inf".to_string() } else { "inf".to_string() };
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0".to_string() } else { "0".to_string() };
    }

    // Decimal exponent via exact-ish f64 scaling.
    let mut e: i32 = 0;
    let mut m = (v.abs()) as f64;
    if m >= 1.0 {
        while m >= 10.0 {
            m /= 10.0;
            e += 1;
        }
    } else {
        while m < 1.0 {
            m *= 10.0;
            e -= 1;
        }
    }

    let strip = |mut s: String| {
        if s.contains('.') {
            while s.ends_with('0') {
                s.pop();
            }
            if s.ends_with('.') {
                s.pop();
            }
        }
        s
    };

    let body = if (-4..6).contains(&e) {
        // Fixed notation with P-1-X decimals.
        let prec = (5 - e).max(0) as usize;
        strip(format!("{:.*}", prec, v.abs() as f64))
    } else {
        // Scientific notation `d.ddddde±XX`.
        let mut mant = strip(format!("{:.5}", m));
        // Rounding could push the mantissa to 10.0 — renormalize.
        if mant.starts_with("10") {
            mant = "1".to_string();
            e += 1;
        }
        let sign = if e < 0 { '-' } else { '+' };
        format!("{}e{}{:02}", mant, sign, e.abs())
    };

    if v < 0.0 {
        format!("-{body}")
    } else {
        body
    }
}

/// Export helpers: CSV heatmaps, PNG heatmaps, region overlays.
///
/// C++ lineage: `class Visualizer` (all methods static; in Rust they are
/// associated functions).
pub struct Visualizer;

impl Visualizer {
    /// CSV heatmap: row-major cells, one float per line (comma header)
    /// (C++ `export_heatmap_csv`).
    pub fn export_heatmap_csv(
        world: &FieldMap,
        field: Field,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let mut out = String::with_capacity(world.cell_count() * 12 + 32);
        out.push_str("x,y,");
        out.push_str(field.name());
        out.push('\n');
        for y in 0..world.height() {
            for x in 0..world.width() {
                let v = world.get(field, x, y)?;
                out.push_str(&format!("{},{},{}\n", x, y, format_g6(v)));
            }
        }
        std::fs::write(path.as_ref(), out).map_err(|e| {
            Error::io(format!(
                "eafar: cannot open {} for CSV export ({e})",
                path.as_ref().display()
            ))
        })
    }

    /// PNG heatmap, value → color via a blue→green→yellow→red ramp.
    /// `min_value`/`max_value` map to the color ramp endpoints
    /// (C++ `export_heatmap_png`, defaults were 0 and 1).
    pub fn export_heatmap_png(
        world: &FieldMap,
        field: Field,
        path: impl AsRef<Path>,
        min_value: f32,
        max_value: f32,
    ) -> Result<()> {
        let rgba = field_to_rgba(world, field, min_value, max_value, None)?;
        Self::write_png_bytes(&rgba, world.width(), world.height(), path.as_ref())
    }

    /// PNG with active regions outlined over the field heatmap
    /// (C++ `export_regions_png`).
    pub fn export_regions_png(
        world: &FieldMap,
        sched: &RegionScheduler,
        field: Field,
        path: impl AsRef<Path>,
        min_value: f32,
        max_value: f32,
    ) -> Result<()> {
        let rgba = field_to_rgba(world, field, min_value, max_value, Some(sched))?;
        Self::write_png_bytes(&rgba, world.width(), world.height(), path.as_ref())
    }

    /// Cells that belong to no active region (C++ `sleeping_cells`).
    pub fn sleeping_cells(world: &FieldMap, sched: &RegionScheduler) -> u64 {
        let mut sleeping = 0u64;
        for y in 0..world.height() {
            for x in 0..world.width() {
                if !sched.is_active_id(sched.region_id(x, y)) {
                    sleeping += 1;
                }
            }
        }
        sleeping
    }

    /// Write raw RGBA bytes as PNG (no world needed) — used by tests too
    /// (C++ `write_rgba_png`).
    pub fn write_rgba_png(
        w: u32,
        h: u32,
        rgba: &[u8],
        path: impl AsRef<Path>,
    ) -> Result<()> {
        Self::write_png_bytes(rgba, w, h, path.as_ref())
    }

    /// Shared file-writing tail of all PNG exporters.
    fn write_png_bytes(rgba: &[u8], w: u32, h: u32, path: &Path) -> Result<()> {
        let png = build_png(w, h, rgba);
        std::fs::write(path, png).map_err(|e| {
            Error::io(format!(
                "eafar: cannot open {} for PNG export ({e})",
                path.display()
            ))
        })
    }
}

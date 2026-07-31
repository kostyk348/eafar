#include "eafar/visualizer.hpp"

#include <cstdio>
#include <fstream>
#include <vector>

namespace eafar {

namespace {

// ---------------------------------------------------------------------------
// Dependency-free PNG writer (stored-deflate, RGB 8-bit).
// Produces valid PNG files readable by any viewer; just not compressed.
// ---------------------------------------------------------------------------

constexpr std::uint8_t kPngSignature[8] = {0x89, 0x50, 0x4E, 0x47,
                                           0x0D, 0x0A, 0x1A, 0x0A};

std::uint32_t crc32(const std::vector<std::uint8_t>& data) {
    static std::uint32_t table[256];
    static bool init = false;
    if (!init) {
        for (std::uint32_t n = 0; n < 256; ++n) {
            std::uint32_t c = n;
            for (int k = 0; k < 8; ++k) {
                c = (c & 1) ? (0xEDB88320u ^ (c >> 1)) : (c >> 1);
            }
            table[n] = c;
        }
        init = true;
    }
    std::uint32_t c = 0xFFFFFFFFu;
    for (std::uint8_t b : data) {
        c = table[(c ^ b) & 0xFFu] ^ (c >> 8);
    }
    return c ^ 0xFFFFFFFFu;
}

void append_be32(std::vector<std::uint8_t>& out, std::uint32_t v) {
    out.push_back(static_cast<std::uint8_t>((v >> 24) & 0xFF));
    out.push_back(static_cast<std::uint8_t>((v >> 16) & 0xFF));
    out.push_back(static_cast<std::uint8_t>((v >> 8) & 0xFF));
    out.push_back(static_cast<std::uint8_t>(v & 0xFF));
}

void write_chunk(std::vector<std::uint8_t>& out, const char type[4],
                 const std::vector<std::uint8_t>& payload) {
    append_be32(out, static_cast<std::uint32_t>(payload.size()));
    const std::size_t type_pos = out.size();
    for (int i = 0; i < 4; ++i) {
        out.push_back(static_cast<std::uint8_t>(type[i]));
    }
    out.insert(out.end(), payload.begin(), payload.end());
    std::vector<std::uint8_t> crc_input(out.begin() + type_pos, out.end());
    append_be32(out, crc32(crc_input));
}

// Adler-32 over the raw (unfiltered-per-scanline-merged) data stream as PNG
// requires: checksum over the scanlines WITH filter bytes.
std::uint32_t adler32(const std::vector<std::uint8_t>& data) {
    std::uint32_t s1 = 1, s2 = 0;
    for (std::uint8_t b : data) {
        s1 = (s1 + b) % 65521u;
        s2 = (s2 + s1) % 65521u;
    }
    return (s2 << 16) | s1;
}

void build_png(std::uint32_t w, std::uint32_t h,
               const std::vector<std::uint8_t>& rgba,
               std::vector<std::uint8_t>& out) {
    out.assign(kPngSignature, kPngSignature + 8);

    // IHDR: 8-bit RGB (color type 2).
    std::vector<std::uint8_t> ihdr;
    append_be32(ihdr, w);
    append_be32(ihdr, h);
    ihdr.push_back(8);              // bit depth
    ihdr.push_back(2);              // color type RGB
    ihdr.push_back(0);              // compression
    ihdr.push_back(0);              // filter
    ihdr.push_back(0);              // interlace
    write_chunk(out, "IHDR", ihdr);

    // Scanlines with filter byte 0 (None).
    std::vector<std::uint8_t> raw;
    raw.reserve(static_cast<std::size_t>(h) * (w * 3 + 1));
    for (std::uint32_t y = 0; y < h; ++y) {
        raw.push_back(0);
        for (std::uint32_t x = 0; x < w; ++x) {
            const std::size_t i = (static_cast<std::size_t>(y) * w + x) * 4;
            raw.push_back(rgba[i]);
            raw.push_back(rgba[i + 1]);
            raw.push_back(rgba[i + 2]);
        }
    }

    // zlib stream: header 0x78 0x01 (no compression, dict off) + stored
    // deflate blocks + adler32 of raw data.
    std::vector<std::uint8_t> idat_payload;
    idat_payload.push_back(0x78);
    idat_payload.push_back(0x01);

    std::size_t pos = 0;
    const std::size_t raw_size = raw.size();
    do {
        const std::size_t block = std::min<std::size_t>(raw_size - pos, 65535);
        const bool last = (pos + block == raw_size);
        idat_payload.push_back(last ? 0x01 : 0x00); // BFINAL|BTYPE=stored
        idat_payload.push_back(static_cast<std::uint8_t>(block & 0xFF));
        idat_payload.push_back(static_cast<std::uint8_t>((block >> 8) & 0xFF));
        idat_payload.push_back(static_cast<std::uint8_t>((~block) & 0xFF));
        idat_payload.push_back(static_cast<std::uint8_t>((~block >> 8) & 0xFF));
        idat_payload.insert(idat_payload.end(), raw.begin() + pos,
                            raw.begin() + pos + block);
        pos += block;
    } while (pos < raw_size);

    const std::uint32_t ad = adler32(raw);
    append_be32(idat_payload, ad);
    write_chunk(out, "IDAT", idat_payload);

    write_chunk(out, "IEND", {});
}

// Blue -> green -> yellow -> red ramp for value in [0,1].
void heat_color(float t, std::uint8_t& r, std::uint8_t& g, std::uint8_t& b) {
    if (t < 0.0f) t = 0.0f;
    if (t > 1.0f) t = 1.0f;
    const float seg = t * 3.0f;
    if (seg < 1.0f) {
        // blue -> cyan
        r = 0;
        g = static_cast<std::uint8_t>(seg * 255.0f);
        b = 255;
    } else if (seg < 2.0f) {
        // cyan -> yellow
        r = static_cast<std::uint8_t>((seg - 1.0f) * 255.0f);
        g = 255;
        b = static_cast<std::uint8_t>((2.0f - seg) * 255.0f);
    } else {
        // yellow -> red
        r = 255;
        g = static_cast<std::uint8_t>((3.0f - seg) * 255.0f);
        b = 0;
    }
}

std::vector<std::uint8_t> field_to_rgba(const FieldMap& world, Field field,
                                        float min_value, float max_value,
                                        const RegionScheduler* overlay) {
    const std::uint32_t w = world.width(), h = world.height();
    const float span = (max_value > min_value) ? (max_value - min_value) : 1.0f;
    const std::uint32_t rs = overlay ? overlay->region_size() : 1;

    std::vector<std::uint8_t> rgba(static_cast<std::size_t>(w) * h * 4);
    for (std::uint32_t y = 0; y < h; ++y) {
        for (std::uint32_t x = 0; x < w; ++x) {
            const std::size_t i = (static_cast<std::size_t>(y) * w + x) * 4;
            const float v = world.get(field, x, y);
            const float t = (v - min_value) / span;

            // If a region boundary passes between this cell and its east or
            // south neighbor, draw a white 1px border.
            bool border = false;
            if (overlay) {
                const bool active = overlay->is_active_id(overlay->region_id(x, y));
                const bool east = (x + 1 < w)
                                      ? overlay->is_active_id(overlay->region_id(x + 1, y))
                                      : false;
                const bool south = (y + 1 < h)
                                       ? overlay->is_active_id(overlay->region_id(x, y + 1))
                                       : false;
                border = (active != east) || (active != south);
                (void)rs;
            }

            if (border) {
                rgba[i] = 255; rgba[i + 1] = 255; rgba[i + 2] = 255; rgba[i + 3] = 255;
            } else {
                heat_color(t, rgba[i], rgba[i + 1], rgba[i + 2]);
                rgba[i + 3] = 255;
            }
        }
    }
    return rgba;
}

} // namespace

void Visualizer::export_heatmap_csv(const FieldMap& world, Field field,
                                    const std::string& path) {
    std::ofstream out(path, std::ios::trunc);
    if (!out) {
        throw std::runtime_error("eafar: cannot open " + path + " for CSV export");
    }
    out << "x,y," << std::string(field_name(field)) << "\n";
    for (std::uint32_t y = 0; y < world.height(); ++y) {
        for (std::uint32_t x = 0; x < world.width(); ++x) {
            out << x << "," << y << "," << world.get(field, x, y) << "\n";
        }
    }
}

void Visualizer::export_heatmap_png(const FieldMap& world, Field field,
                                    const std::string& path, float min_value,
                                    float max_value) {
    const auto rgba = field_to_rgba(world, field, min_value, max_value, nullptr);
    std::vector<std::uint8_t> png;
    build_png(world.width(), world.height(), rgba, png);

    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out) {
        throw std::runtime_error("eafar: cannot open " + path + " for PNG export");
    }
    out.write(reinterpret_cast<const char*>(png.data()),
              static_cast<std::streamsize>(png.size()));
}

void Visualizer::export_regions_png(const FieldMap& world,
                                    const RegionScheduler& sched, Field field,
                                    const std::string& path, float min_value,
                                    float max_value) {
    const auto rgba = field_to_rgba(world, field, min_value, max_value, &sched);
    std::vector<std::uint8_t> png;
    build_png(world.width(), world.height(), rgba, png);

    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out) {
        throw std::runtime_error("eafar: cannot open " + path + " for PNG export");
    }
    out.write(reinterpret_cast<const char*>(png.data()),
              static_cast<std::streamsize>(png.size()));
}

std::uint64_t Visualizer::sleeping_cells(const FieldMap& world,
                                         const RegionScheduler& sched) {
    std::uint64_t sleeping = 0;
    for (std::uint32_t y = 0; y < world.height(); ++y) {
        for (std::uint32_t x = 0; x < world.width(); ++x) {
            if (!sched.is_active_id(sched.region_id(x, y))) {
                ++sleeping;
            }
        }
    }
    return sleeping;
}

void Visualizer::write_rgba_png(std::uint32_t w, std::uint32_t h,
                                const std::vector<std::uint8_t>& rgba,
                                const std::string& path) {
    std::vector<std::uint8_t> png;
    build_png(w, h, rgba, png);
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out) {
        throw std::runtime_error("eafar: cannot open " + path + " for PNG export");
    }
    out.write(reinterpret_cast<const char*>(png.data()),
              static_cast<std::streamsize>(png.size()));
}

} // namespace eafar

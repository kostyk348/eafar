#pragma once
// eafar/visualizer.hpp — S10: Visualization API.
//
// No GUI, no OpenGL — just exports that let any external tool render:
//   * CSV  — one heatmap per file, machine-readable
//   * PNG  — dependency-free heatmap renderer (stored-deflate PNG writer)
//   * overlay of active regions on top of a field
//   * sleeping-cell statistics
//
// Output is deterministic: same world -> same bytes.

#include "eafar/field_map.hpp"
#include "eafar/region_scheduler.hpp"

#include <cstdint>
#include <string>

namespace eafar {

class Visualizer {
public:
    // CSV heatmap: row-major cells, one float per line (comma header).
    static void export_heatmap_csv(const FieldMap& world, Field field,
                                   const std::string& path);

    // PNG heatmap, value -> color via a blue->green->yellow->red ramp.
    // min/max map to the color ramp endpoints (defaults: 0 and 1).
    static void export_heatmap_png(const FieldMap& world, Field field,
                                   const std::string& path,
                                   float min_value = 0.0f,
                                   float max_value = 1.0f);

    // PNG with active regions outlined over the field heatmap.
    static void export_regions_png(const FieldMap& world,
                                   const RegionScheduler& sched, Field field,
                                   const std::string& path,
                                   float min_value = 0.0f,
                                   float max_value = 1.0f);

    // Cells that belong to no active region.
    static std::uint64_t sleeping_cells(const FieldMap& world,
                                        const RegionScheduler& sched);

    // Write raw RGBA bytes as PNG (no world needed) — used by tests too.
    static void write_rgba_png(std::uint32_t w, std::uint32_t h,
                               const std::vector<std::uint8_t>& rgba,
                               const std::string& path);
};

} // namespace eafar

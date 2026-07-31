#include "test_runner.hpp"
#include "eafar/visualizer.hpp"
#include "eafar/region_scheduler.hpp"

#include <cstdint>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

using namespace eafar;

// Test outputs land in a temp dir next to the binary.
static std::string out_path(const char* name) {
    return std::string("viz_") + name;
}

static std::vector<std::uint8_t> read_file(const std::string& path) {
    std::ifstream in(path, std::ios::binary);
    CHECK(in.good());
    return std::vector<std::uint8_t>((std::istreambuf_iterator<char>(in)),
                                     std::istreambuf_iterator<char>());
}

TEST(csv_heatmap_export) {
    FieldMap world(4, 4);
    world.set(Field::Temperature, 1, 2, 0.75f);

    const std::string path = out_path("heat.csv");
    Visualizer::export_heatmap_csv(world, Field::Temperature, path);

    std::ifstream in(path);
    CHECK(in.good());
    std::stringstream ss;
    ss << in.rdbuf();
    const std::string content = ss.str();

    CHECK(content.find("x,y,Temperature") != std::string::npos);
    CHECK(content.find("1,2,0.75") != std::string::npos); // the hot cell
    // 4x4 = 16 data rows + 1 header.
    std::size_t lines = 0;
    for (char c : content) {
        if (c == '\n') ++lines;
    }
    CHECK_EQ(lines, 17u);
}

TEST(png_heatmap_export_valid_signature) {
    FieldMap world(8, 8);
    world.set(Field::Radiation, 4, 4, 1.0f);

    const std::string path = out_path("heat.png");
    Visualizer::export_heatmap_png(world, Field::Radiation, path);

    const auto bytes = read_file(path);
    CHECK(bytes.size() > 64); // not empty

    // PNG signature.
    const std::uint8_t sig[8] = {0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A};
    CHECK(std::equal(sig, sig + 8, bytes.begin()));

    // IHDR present with correct dimensions (8x8, 8-bit, RGB).
    CHECK(bytes[12] == 'I' && bytes[13] == 'H' && bytes[14] == 'D' && bytes[15] == 'R');
    CHECK_EQ(bytes[16], 0u); // width high byte
    CHECK_EQ(bytes[20], 0u); // height high byte
    CHECK_EQ(bytes[24], 8u); // bit depth
    CHECK_EQ(bytes[25], 2u); // color type RGB

    // IDAT present; IEND is the last chunk (len4 + "IEND"4 + crc4 = 12 bytes).
    const std::vector<std::uint8_t> iend_marker = {'I', 'E', 'N', 'D'};
    const auto iend = std::search(bytes.begin(), bytes.end(),
                                  iend_marker.begin(), iend_marker.end());
    CHECK(iend != bytes.end());
    CHECK(std::size_t(std::distance(iend, bytes.end())) == 8u); // crc(4)+len(4) after "IEND"
    // The chunk length field before IEND says 0 (empty payload).
    const std::size_t len_pos = static_cast<std::size_t>(iend - bytes.begin()) - 4;
    CHECK_EQ(bytes[len_pos], 0u);
    CHECK_EQ(bytes[len_pos + 1], 0u);
    CHECK_EQ(bytes[len_pos + 2], 0u);
    CHECK_EQ(bytes[len_pos + 3], 0u);
}

TEST(png_export_deterministic_bytes) {
    FieldMap world(6, 6);
    world.set(Field::Temperature, 2, 2, 0.5f);

    const std::string p1 = out_path("det1.png");
    const std::string p2 = out_path("det2.png");
    Visualizer::export_heatmap_png(world, Field::Temperature, p1);
    Visualizer::export_heatmap_png(world, Field::Temperature, p2);

    const auto a = read_file(p1);
    const auto b = read_file(p2);
    CHECK(a == b); // deterministic: same world -> identical bytes
}

TEST(regions_png_overlay) {
    FieldMap world(64, 64);
    world.set(Field::Temperature, 40, 40, 1.0f); // hot cell in region (1,1)

    RegionScheduler sched(64, 64, 32);
    sched.set_idle_ticks_before_sleep(1 << 30);
    sched.wake_region(1, 1);

    const std::string path = out_path("regions.png");
    Visualizer::export_regions_png(world, sched, Field::Temperature, path);

    const auto bytes = read_file(path);
    CHECK(bytes.size() > 64);
    const std::uint8_t sig[8] = {0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A};
    CHECK(std::equal(sig, sig + 8, bytes.begin()));
}

TEST(sleeping_cell_count) {
    FieldMap world(64, 64); // 2x2 regions of 32x32 = 4096 cells
    RegionScheduler sched(64, 64, 32);
    sched.set_idle_ticks_before_sleep(1 << 30);

    CHECK_EQ(Visualizer::sleeping_cells(world, sched), 4096u); // all asleep

    sched.wake_region(0, 0);
    CHECK_EQ(Visualizer::sleeping_cells(world, sched), 4096u - 32 * 32); // 3072

    sched.wake_region(1, 1);
    CHECK_EQ(Visualizer::sleeping_cells(world, sched), 2048u); // half awake
}

TEST(write_rgba_png_direct) {
    // 1x1 red pixel PNG.
    std::vector<std::uint8_t> rgba = {255, 0, 0, 255};
    const std::string path = out_path("pixel.png");
    Visualizer::write_rgba_png(1, 1, rgba, path);

    const auto bytes = read_file(path);
    CHECK(bytes.size() > 64);
    // IHDR width=1 height=1.
    CHECK_EQ(bytes[16], 0u);
    CHECK_EQ(bytes[17], 0u);
    CHECK_EQ(bytes[18], 0u);
    CHECK_EQ(bytes[19], 1u);
    CHECK_EQ(bytes[20], 0u);
    CHECK_EQ(bytes[21], 0u);
    CHECK_EQ(bytes[22], 0u);
    CHECK_EQ(bytes[23], 1u);
}

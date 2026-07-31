#include "eafar/region_scheduler.hpp"

#include <algorithm>
#include <stdexcept>

namespace eafar {

RegionScheduler::RegionScheduler(std::uint32_t world_w, std::uint32_t world_h,
                                 std::uint32_t region_size)
    : region_size_(region_size) {
    if (region_size == 0) {
        throw std::invalid_argument("eafar: region_size must be > 0");
    }
    // Ceil division: a world not divisible by region_size gets partial edge regions.
    regions_x_ = (world_w + region_size - 1) / region_size;
    regions_y_ = (world_h + region_size - 1) / region_size;
}

std::uint32_t RegionScheduler::region_x(std::uint32_t wx) const noexcept {
    return wx / region_size_;
}

std::uint32_t RegionScheduler::region_y(std::uint32_t wy) const noexcept {
    return wy / region_size_;
}

std::uint32_t RegionScheduler::region_id(std::uint32_t wx, std::uint32_t wy) const noexcept {
    return region_y(wy) * regions_x_ + region_x(wx);
}

bool RegionScheduler::is_active(std::uint32_t rx, std::uint32_t ry) const noexcept {
    if (rx >= regions_x_ || ry >= regions_y_) {
        return false;
    }
    return is_active_id(ry * regions_x_ + rx);
}

bool RegionScheduler::is_active_id(std::uint32_t id) const noexcept {
    return active_.find(id) != active_.end();
}

void RegionScheduler::wake(std::uint32_t wx, std::uint32_t wy) {
    wake_region(region_x(wx), region_y(wy));
}

void RegionScheduler::wake_region(std::uint32_t rx, std::uint32_t ry) {
    if (rx >= regions_x_ || ry >= regions_y_) {
        throw std::out_of_range("eafar: wake_region out of bounds");
    }
    const std::uint32_t id = ry * regions_x_ + rx;
    active_.insert(id);
    idle_ticks_[id] = 0; // wake resets the idle counter
    woken_during_work_.insert(id); // matters only if called during tick's work phase
}

void RegionScheduler::sleep_region(std::uint32_t rx, std::uint32_t ry) {
    if (rx >= regions_x_ || ry >= regions_y_) {
        throw std::out_of_range("eafar: sleep_region out of bounds");
    }
    const std::uint32_t id = ry * regions_x_ + rx;
    active_.erase(id);
    idle_ticks_.erase(id);
}

void RegionScheduler::set_region_work(RegionWork work) {
    work_ = std::move(work);
}

void RegionScheduler::tick(FieldMap& world) {
    // Iterate active regions in ascending id order (deterministic).
    // Work may wake new regions or re-wake the current one, so snapshot first.
    std::vector<std::uint32_t> snapshot(active_.begin(), active_.end());
    woken_during_work_.clear();

    for (std::uint32_t id : snapshot) {
        if (work_) {
            const std::uint32_t rx = id % regions_x_;
            const std::uint32_t ry = id / regions_x_;
            work_(rx, ry, world);
        }

        // A region re-woken during work gets a fresh lifetime (idle reset);
        // it is not aged this tick.
        if (woken_during_work_.count(id)) {
            continue;
        }

        auto it = idle_ticks_.find(id);
        if (it == idle_ticks_.end()) {
            continue; // work slept the region; nothing to age
        }
        ++it->second;
        if (it->second >= idle_before_sleep_) {
            active_.erase(id);
            idle_ticks_.erase(it);
        }
    }
}

} // namespace eafar

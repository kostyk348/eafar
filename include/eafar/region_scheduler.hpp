#pragma once
// eafar/region_scheduler.hpp — S3: Region Scheduler.
//
// THE core thesis of EAFAR: an event must not wake the whole world, only
// its region. The world is partitioned into fixed-size regions; each region
// is either SLEEPING (zero work) or ACTIVE (local computation runs).
//
// Lifecycle per region: Event -> wake -> local computation (tick) ->
// idle for N ticks -> sleep.
//
// Determinism: active regions are iterated in ascending region-id order
// (ordered containers only — no unordered_map/set anywhere).

#include "eafar/field_map.hpp"

#include <cstdint>
#include <functional>
#include <map>
#include <set>
#include <unordered_set>

namespace eafar {

class RegionScheduler {
public:
    // world_w x world_h world cells, partitioned into region_size x region_size regions.
    RegionScheduler(std::uint32_t world_w, std::uint32_t world_h,
                    std::uint32_t region_size = 32);

    // --- geometry ---
    std::uint32_t region_size() const noexcept { return region_size_; }
    std::uint32_t regions_x() const noexcept { return regions_x_; }
    std::uint32_t regions_y() const noexcept { return regions_y_; }
    std::uint32_t region_count() const noexcept { return regions_x_ * regions_y_; }

    // world coord -> region id (row-major)
    std::uint32_t region_id(std::uint32_t wx, std::uint32_t wy) const noexcept;
    std::uint32_t region_x(std::uint32_t wx) const noexcept;
    std::uint32_t region_y(std::uint32_t wy) const noexcept;

    // --- state ---
    bool is_active(std::uint32_t rx, std::uint32_t ry) const noexcept;
    bool is_active_id(std::uint32_t id) const noexcept;
    std::size_t active_count() const noexcept { return active_.size(); }

    // --- activation ---
    void wake(std::uint32_t wx, std::uint32_t wy);      // world coords
    void wake_region(std::uint32_t rx, std::uint32_t ry);
    void sleep_region(std::uint32_t rx, std::uint32_t ry);

    // --- work ---
    // Per-tick local computation for one ACTIVE region. Receives the region
    // coordinates and the world. Runs only on active regions.
    using RegionWork = std::function<void(std::uint32_t rx, std::uint32_t ry,
                                          FieldMap& world)>;
    void set_region_work(RegionWork work);

    // --- ticking ---
    // Advance one tick: run work on every active region (ascending id order),
    // then age idle regions; regions idle >= idle_ticks_before_sleep sleep.
    void tick(FieldMap& world);

    std::uint32_t idle_ticks_before_sleep() const noexcept { return idle_before_sleep_; }
    void set_idle_ticks_before_sleep(std::uint32_t n) noexcept { idle_before_sleep_ = n; }

    // Deterministic snapshot of active region ids (ascending).
    const std::set<std::uint32_t>& active_ids() const noexcept { return active_; }

private:
    std::uint32_t region_size_;
    std::uint32_t regions_x_;
    std::uint32_t regions_y_;
    std::uint32_t idle_before_sleep_ = 1;

    std::set<std::uint32_t> active_;                    // active region ids, ascending
    std::map<std::uint32_t, std::uint32_t> idle_ticks_; // region id -> ticks since last wake
    RegionWork work_;

    // Regions woken during the current tick's work phase. Used to grant a
    // fresh lifetime (skip aging) to regions re-woken by their own work.
    // NOTE: temporary scratch only — not part of the deterministic state;
    // cleared at the start of every tick().
    std::set<std::uint32_t> woken_during_work_;
};

} // namespace eafar

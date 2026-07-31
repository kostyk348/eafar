#pragma once
// eafar/automaton.hpp — S4: Asynchronous Automata.
//
// Every automaton lives on its own clock: it runs every `period_ticks`
// base simulation ticks. Fire at 60Hz -> period 1, Water at 10Hz -> 6,
// Plants at 0.1Hz -> 600, Evolution at 1/hour -> 216000 (at 60 ticks/sec).
//
// An automaton step is executed per ACTIVE region (region locality from S3),
// and reads/writes only fields (coupling rule from S1/S6) — it never
// references other automata. All iteration order is fixed (registration
// order, ascending region id) => deterministic.

#include "eafar/field_map.hpp"
#include "eafar/region_scheduler.hpp"

#include <cstdint>
#include <functional>
#include <stdexcept>
#include <string>
#include <vector>

namespace eafar {

class AutomatonManager {
public:
    using Step = std::function<void(std::uint32_t rx, std::uint32_t ry,
                                    FieldMap& world)>;

    struct Automaton {
        std::string name;
        std::uint64_t period;   // run every N base ticks
        std::uint64_t executed = 0; // times the step ran (due ticks)
        Step step;
    };

    // Register an automaton. period must be >= 1.
    std::size_t add(std::string name, std::uint64_t period, Step step);

    // Advance the simulation by one base tick. Every due automaton runs its
    // step over every ACTIVE region (ascending region id). Deterministic.
    void advance(FieldMap& world, RegionScheduler& sched);

    std::uint64_t sim_tick() const noexcept { return sim_tick_; }
    std::size_t count() const noexcept { return automata_.size(); }

    const Automaton& at(std::size_t idx) const { return automata_.at(idx); }
    const std::vector<Automaton>& automata() const noexcept { return automata_; }

private:
    std::vector<Automaton> automata_;
    std::uint64_t sim_tick_ = 0;
};

} // namespace eafar

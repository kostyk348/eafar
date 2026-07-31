#include "eafar/automaton.hpp"

namespace eafar {

std::size_t AutomatonManager::add(std::string name, std::uint64_t period,
                                  Step step) {
    if (period == 0) {
        throw std::invalid_argument("eafar: automaton period must be >= 1");
    }
    automata_.push_back(Automaton{std::move(name), period, 0, std::move(step)});
    return automata_.size() - 1;
}

void AutomatonManager::advance(FieldMap& world, RegionScheduler& sched) {
    ++sim_tick_;
    for (auto& a : automata_) {
        if (sim_tick_ % a.period != 0) {
            continue;
        }
        // Per active region, ascending id (deterministic).
        for (std::uint32_t id : sched.active_ids()) {
            const std::uint32_t rx = id % sched.regions_x();
            const std::uint32_t ry = id / sched.regions_x();
            a.step(rx, ry, world);
        }
        ++a.executed;
    }
}

} // namespace eafar

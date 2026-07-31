#include "eafar/dependency.hpp"

#include <algorithm>
#include <sstream>
#include <stdexcept>

namespace eafar {

void DependencyGraph::declare(std::string automaton,
                              std::vector<FieldAccess> accesses) {
    // Remove any previous declaration of this automaton.
    auto old = std::remove_if(
        edges_.begin(), edges_.end(),
        [&](const DepEdge& e) { return e.automaton == automaton; });
    edges_.erase(old, edges_.end());

    for (auto& a : accesses) {
        edges_.push_back(DepEdge{automaton, a.field, a.access});
    }

    auto existing = std::find_if(
        by_automaton_.begin(), by_automaton_.end(),
        [&](const auto& p) { return p.first == automaton; });
    if (existing != by_automaton_.end()) {
        existing->second = std::move(accesses);
    } else {
        by_automaton_.emplace_back(std::move(automaton), std::move(accesses));
    }
}

bool DependencyGraph::has(std::string_view automaton) const noexcept {
    return std::any_of(by_automaton_.begin(), by_automaton_.end(),
                       [&](const auto& p) { return p.first == automaton; });
}

const std::vector<FieldAccess>& DependencyGraph::accesses_of(
    std::string_view automaton) const {
    auto it = std::find_if(by_automaton_.begin(), by_automaton_.end(),
                           [&](const auto& p) { return p.first == automaton; });
    if (it == by_automaton_.end()) {
        throw std::out_of_range("eafar: unknown automaton in dependency graph");
    }
    return it->second;
}

bool DependencyGraph::validate() const noexcept {
    // By construction every edge is automaton -> field. This is the check
    // that catches (at tooling time) any attempt to couple automata.
    return std::all_of(edges_.begin(), edges_.end(), [](const DepEdge& e) {
        return e.field >= Field::Temperature && e.field < Field::_count;
    });
}

std::string DependencyGraph::dump() const {
    std::ostringstream out;
    for (const DepEdge& e : edges_) {
        out << e.automaton << ' '
            << (e.access == Access::Read ? "reads" : "writes") << ' '
            << field_name(e.field) << '\n';
    }
    return out.str();
}

} // namespace eafar

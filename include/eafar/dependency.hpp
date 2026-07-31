#pragma once
// eafar/dependency.hpp — S6: Dependency Graph.
//
// Automata never know about each other. The only connection between an
// automaton and the world is a declared set of FIELD accesses:
//
//     Fire  writes Temperature
//     Plants reads  Temperature
//
// The graph is introspectable (dump / edges) so tools can verify that no
// automaton->automaton edge exists anywhere — coupling is field-only.

#include "eafar/field.hpp"

#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

namespace eafar {

enum class Access : std::uint8_t { Read, Write };

struct FieldAccess {
    Field field;
    Access access;
};

struct DepEdge {
    std::string automaton; // source: automaton
    Field field;           // target: field
    Access access;         // direction: reads / writes
};

class DependencyGraph {
public:
    // Declare an automaton's field accesses. Re-declaring replaces.
    void declare(std::string automaton, std::vector<FieldAccess> accesses);

    bool has(std::string_view automaton) const noexcept;
    std::size_t automaton_count() const noexcept { return by_automaton_.size(); }
    std::size_t edge_count() const noexcept { return edges_.size(); }

    const std::vector<FieldAccess>& accesses_of(std::string_view automaton) const;
    const std::vector<DepEdge>& edges() const noexcept { return edges_; }

    // True if every declared edge is automaton->field (structural invariant:
    // there is no edge whose target is an automaton). Always holds by
    // construction — kept as an explicit check for tooling.
    bool validate() const noexcept;

    // Textual dump, one edge per line:
    //   "Fire writes Temperature"
    std::string dump() const;

private:
    std::vector<DepEdge> edges_;
    std::vector<std::pair<std::string, std::vector<FieldAccess>>> by_automaton_;
};

} // namespace eafar

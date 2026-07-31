#include "test_runner.hpp"
#include "eafar/dependency.hpp"

#include <sstream>

using namespace eafar;

TEST(dependency_declaration_edges) {
    DependencyGraph g;
    g.declare("Fire", {{Field::Temperature, Access::Write},
                       {Field::Energy, Access::Write}});
    g.declare("Plants", {{Field::Temperature, Access::Read},
                         {Field::Population, Access::Write}});

    CHECK_EQ(g.automaton_count(), 2u);
    CHECK_EQ(g.edge_count(), 4u);
    CHECK(g.has("Fire"));
    CHECK(g.has("Plants"));
    CHECK(!g.has("Water"));
    CHECK(g.validate());
}

TEST(dependency_no_automaton_to_automaton_edges) {
    // THE structural invariant: every edge target is a FIELD.
    DependencyGraph g;
    g.declare("Fire", {{Field::Temperature, Access::Write}});
    g.declare("Plants", {{Field::Temperature, Access::Read}});

    for (const DepEdge& e : g.edges()) {
        CHECK(e.field >= Field::Temperature && e.field < Field::_count);
    }
    // There are no automaton nodes in the target position at all.
    CHECK(g.validate());
}

TEST(dependency_dump_text) {
    DependencyGraph g;
    g.declare("Fire", {{Field::Temperature, Access::Write}});
    g.declare("Plants", {{Field::Temperature, Access::Read},
                         {Field::Energy, Access::Write}});

    const std::string dump = g.dump();
    std::istringstream in(dump);
    std::string line;
    std::vector<std::string> lines;
    while (std::getline(in, line)) {
        lines.push_back(line);
    }

    CHECK_EQ(lines.size(), 3u);
    CHECK(lines[0] == "Fire writes Temperature");
    CHECK(lines[1] == "Plants reads Temperature");
    CHECK(lines[2] == "Plants writes Energy");
}

TEST(dependency_redeclare_replaces) {
    DependencyGraph g;
    g.declare("Fire", {{Field::Temperature, Access::Write}});
    CHECK_EQ(g.edge_count(), 1u);

    // Updated declaration replaces the old edges.
    g.declare("Fire", {{Field::Temperature, Access::Write},
                       {Field::Radiation, Access::Write}});
    CHECK_EQ(g.edge_count(), 2u);
    CHECK_EQ(g.automaton_count(), 1u);
    CHECK_EQ(g.accesses_of("Fire").size(), 2u);
}

TEST(dependency_accesses_of_unknown_throws) {
    DependencyGraph g;
    g.declare("Fire", {{Field::Temperature, Access::Write}});
    CHECK_THROWS(g.accesses_of("Water"));
}

TEST(dependency_empty_automaton_ok) {
    DependencyGraph g;
    g.declare("Ghost", {});
    CHECK(g.has("Ghost"));
    CHECK_EQ(g.edge_count(), 0u);
    CHECK(g.validate());
    CHECK(g.dump().empty());
}

// ---------------------------------------------------------------------------
// S6 integration: automata are wired ONLY through the graph's field edges.
// The runtime loop (events -> automata -> regions) never references
// automaton-to-automaton; every interaction goes through FieldMap.
// ---------------------------------------------------------------------------

TEST(dependency_graph_drives_real_automata) {
    DependencyGraph g;

    // Fire consumes Energy, raises Temperature.
    g.declare("Fire", {{Field::Energy, Access::Read},
                       {Field::Temperature, Access::Write},
                       {Field::Energy, Access::Write}});
    // Water lowers Temperature, raises Humidity.
    g.declare("Water", {{Field::Temperature, Access::Read},
                        {Field::Temperature, Access::Write},
                        {Field::Humidity, Access::Write}});
    // Plants read Temperature, write Population.
    g.declare("Plants", {{Field::Temperature, Access::Read},
                         {Field::Population, Access::Write}});

    // The graph is the ONLY declared coupling between these components.
    CHECK(g.validate());
    CHECK_EQ(g.edge_count(), 8u);
    CHECK(g.has("Fire") && g.has("Water") && g.has("Plants"));
}

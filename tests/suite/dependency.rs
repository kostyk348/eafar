//! Rust port of `tests/test_dependency.cpp` (7 tests).

use eafar::prelude::*;

#[test]
fn dependency_declaration_edges() {
    let mut g = DependencyGraph::default();
    g.declare(
        "Fire",
        vec![
            FieldAccess {
                field: Field::Temperature,
                access: Access::Write,
            },
            FieldAccess {
                field: Field::Energy,
                access: Access::Write,
            },
        ],
    );
    g.declare(
        "Plants",
        vec![
            FieldAccess {
                field: Field::Temperature,
                access: Access::Read,
            },
            FieldAccess {
                field: Field::Population,
                access: Access::Write,
            },
        ],
    );

    check_eq!(g.automaton_count(), 2);
    check_eq!(g.edge_count(), 4);
    check!(g.has("Fire"));
    check!(g.has("Plants"));
    check!(!g.has("Water"));
    check!(g.validate());
}

#[test]
fn dependency_no_automaton_to_automaton_edges() {
    // THE structural invariant: every edge target is a FIELD.
    let mut g = DependencyGraph::default();
    g.declare(
        "Fire",
        vec![FieldAccess {
            field: Field::Temperature,
            access: Access::Write,
        }],
    );
    g.declare(
        "Plants",
        vec![FieldAccess {
            field: Field::Temperature,
            access: Access::Read,
        }],
    );

    for e in g.edges() {
        check!(Field::ALL.contains(&e.field));
    }
    // There are no automaton nodes in the target position at all.
    check!(g.validate());
}

#[test]
fn dependency_dump_text() {
    let mut g = DependencyGraph::default();
    g.declare(
        "Fire",
        vec![FieldAccess {
            field: Field::Temperature,
            access: Access::Write,
        }],
    );
    g.declare(
        "Plants",
        vec![
            FieldAccess {
                field: Field::Temperature,
                access: Access::Read,
            },
            FieldAccess {
                field: Field::Energy,
                access: Access::Write,
            },
        ],
    );

    let dump = g.dump();
    let lines: Vec<&str> = dump.lines().collect();

    check_eq!(lines.len(), 3);
    check!(lines[0] == "Fire writes Temperature");
    check!(lines[1] == "Plants reads Temperature");
    check!(lines[2] == "Plants writes Energy");
}

#[test]
fn dependency_redeclare_replaces() {
    let mut g = DependencyGraph::default();
    g.declare(
        "Fire",
        vec![FieldAccess {
            field: Field::Temperature,
            access: Access::Write,
        }],
    );
    check_eq!(g.edge_count(), 1);

    // Updated declaration replaces the old edges.
    g.declare(
        "Fire",
        vec![
            FieldAccess {
                field: Field::Temperature,
                access: Access::Write,
            },
            FieldAccess {
                field: Field::Radiation,
                access: Access::Write,
            },
        ],
    );
    check_eq!(g.edge_count(), 2);
    check_eq!(g.automaton_count(), 1);
    check_eq!(g.accesses_of("Fire").unwrap().len(), 2);
}

#[test]
fn dependency_accesses_of_unknown_throws() {
    let mut g = DependencyGraph::default();
    g.declare(
        "Fire",
        vec![FieldAccess {
            field: Field::Temperature,
            access: Access::Write,
        }],
    );
    check_throws!(g.accesses_of("Water"));
}

#[test]
fn dependency_empty_automaton_ok() {
    let mut g = DependencyGraph::default();
    g.declare("Ghost", vec![]);
    check!(g.has("Ghost"));
    check_eq!(g.edge_count(), 0);
    check!(g.validate());
    check!(g.dump().is_empty());
}

// ---------------------------------------------------------------------------
// S6 integration: automata are wired ONLY through the graph's field edges.
// The runtime loop (events -> automata -> regions) never references
// automaton-to-automaton; every interaction goes through FieldMap.
// ---------------------------------------------------------------------------

#[test]
fn dependency_graph_drives_real_automata() {
    let mut g = DependencyGraph::default();

    // Fire consumes Energy, raises Temperature.
    g.declare(
        "Fire",
        vec![
            FieldAccess {
                field: Field::Energy,
                access: Access::Read,
            },
            FieldAccess {
                field: Field::Temperature,
                access: Access::Write,
            },
            FieldAccess {
                field: Field::Energy,
                access: Access::Write,
            },
        ],
    );
    // Water lowers Temperature, raises Humidity.
    g.declare(
        "Water",
        vec![
            FieldAccess {
                field: Field::Temperature,
                access: Access::Read,
            },
            FieldAccess {
                field: Field::Temperature,
                access: Access::Write,
            },
            FieldAccess {
                field: Field::Humidity,
                access: Access::Write,
            },
        ],
    );
    // Plants read Temperature, write Population.
    g.declare(
        "Plants",
        vec![
            FieldAccess {
                field: Field::Temperature,
                access: Access::Read,
            },
            FieldAccess {
                field: Field::Population,
                access: Access::Write,
            },
        ],
    );

    // The graph is the ONLY declared coupling between these components.
    check!(g.validate());
    check_eq!(g.edge_count(), 8);
    check!(g.has("Fire") && g.has("Water") && g.has("Plants"));
}

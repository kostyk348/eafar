//! S6: Dependency Graph.
//!
//! C++ lineage: `include/eafar/dependency.hpp` + `src/dependency.cpp`.
//!
//! Automata never know about each other. The only connection between an
//! automaton and the world is a declared set of FIELD accesses:
//!
//! ```text
//! Fire   writes Temperature
//! Plants reads  Temperature
//! ```
//!
//! The graph is introspectable (dump / edges) so tools can verify that no
//! automaton→automaton edge exists anywhere — coupling is field-only.

use std::fmt::Write as _;

use crate::error::{Error, Result};
use crate::field::{field_name, Field};

/// Direction of a field access (C++ `enum class Access`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Access {
    /// The automaton reads the field.
    Read,
    /// The automaton writes the field.
    Write,
}

/// One declared access of an automaton to a field (C++ `FieldAccess`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldAccess {
    /// Which field.
    pub field: Field,
    /// Read or write.
    pub access: Access,
}

/// One resolved edge of the graph: `automaton -> field` (C++ `DepEdge`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepEdge {
    /// Source: automaton name.
    pub automaton: String,
    /// Target: field.
    pub field: Field,
    /// Direction: reads / writes.
    pub access: Access,
}

/// Structural coupling map of automata to fields.
///
/// C++ lineage: `class DependencyGraph`.
#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    edges: Vec<DepEdge>,
    by_automaton: Vec<(String, Vec<FieldAccess>)>,
}

impl DependencyGraph {
    /// Declare an automaton's field accesses. Re-declaring replaces the
    /// previous declaration (C++ `declare`).
    pub fn declare(&mut self, automaton: impl Into<String>, accesses: Vec<FieldAccess>) {
        let automaton = automaton.into();

        // Remove any previous declaration of this automaton.
        self.edges
            .retain(|e| e.automaton != automaton);

        for a in &accesses {
            self.edges.push(DepEdge {
                automaton: automaton.clone(),
                field: a.field,
                access: a.access,
            });
        }

        match self
            .by_automaton
            .iter_mut()
            .find(|(name, _)| *name == automaton)
        {
            Some((_, existing)) => *existing = accesses,
            None => self.by_automaton.push((automaton, accesses)),
        }
    }

    /// True when the automaton has any declaration (C++ `has`).
    pub fn has(&self, automaton: &str) -> bool {
        self.by_automaton
            .iter()
            .any(|(name, _)| name == automaton)
    }

    /// Number of distinct declared automata (C++ `automaton_count`).
    pub fn automaton_count(&self) -> usize {
        self.by_automaton.len()
    }

    /// Number of resolved edges (C++ `edge_count`).
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Declared accesses of one automaton. Errors when unknown (C++ threw
    /// `std::out_of_range`).
    pub fn accesses_of(&self, automaton: &str) -> Result<&[FieldAccess]> {
        match self
            .by_automaton
            .iter()
            .find(|(name, _)| name == automaton)
        {
            Some((_, accesses)) => Ok(accesses),
            None => Err(Error::not_found(
                "eafar: unknown automaton in dependency graph",
            )),
        }
    }

    /// All resolved edges, in declaration order (C++ `edges`).
    pub fn edges(&self) -> &[DepEdge] {
        &self.edges
    }

    /// True if every declared edge is automaton→field (structural
    /// invariant: there is no edge whose target is an automaton). Always
    /// holds by construction — kept as an explicit check for tooling,
    /// exactly like the C++ `validate`.
    pub fn validate(&self) -> bool {
        self.edges
            .iter()
            .all(|e| Field::ALL.contains(&e.field))
    }

    /// Textual dump, one edge per line:
    /// `Fire writes Temperature` (C++ `dump`).
    pub fn dump(&self) -> String {
        let mut out = String::new();
        for e in &self.edges {
            let _ = writeln!(
                out,
                "{} {} {}",
                e.automaton,
                if e.access == Access::Read { "reads" } else { "writes" },
                field_name(e.field)
            );
        }
        out
    }
}

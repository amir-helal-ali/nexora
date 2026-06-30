//! Dependency graph with acyclic validation.
//!
//! See Nexora Engineering Specification, Part 5 (DEPENDENCY SYSTEM).
//! Packages may depend on other packages, core modules, NXP capabilities,
//! and external APIs (restricted). The dependency graph MUST be acyclic.

use crate::version::{ParseVersionError, VersionRange};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// A single dependency declaration.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Dependency {
    /// Package ID this dependency refers to.
    pub package_id: String,
    /// Version range (e.g. `^1.2.0`).
    pub range: VersionRange,
}

impl Dependency {
    /// Construct a new dependency with the given range string.
    pub fn new(package_id: String, range_str: &str) -> Result<Self, ParseVersionError> {
        Ok(Self {
            package_id,
            range: VersionRange::parse(range_str)?,
        })
    }
}

impl fmt::Display for Dependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.package_id, self.range)
    }
}

/// Error from dependency operations.
#[derive(Debug, thiserror::Error)]
pub enum DependencyError {
    /// A circular dependency was detected.
    #[error("circular dependency detected: {0}")]
    Circular(String),
    /// A required dependency is missing from the registry.
    #[error("missing dependency: {0}")]
    Missing(String),
    /// A dependency's version does not satisfy the required range.
    #[error("version mismatch: {package_id} requires {range}, got {actual}")]
    VersionMismatch {
        /// Package ID.
        package_id: String,
        /// Required range.
        range: String,
        /// Actual version.
        actual: String,
    },
}

/// The dependency graph. Used to validate that a set of packages forms a
/// directed acyclic graph (DAG) and that all version constraints are
/// satisfiable.
#[derive(Default)]
pub struct DependencyGraph {
    /// Map: package ID → list of (dependency ID, version range).
    edges: HashMap<String, Vec<Dependency>>,
}

impl fmt::Debug for DependencyGraph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DependencyGraph")
            .field("nodes", &self.edges.len())
            .finish()
    }
}

impl DependencyGraph {
    /// Construct an empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a node (package) with its declared dependencies.
    pub fn add_node(&mut self, package_id: impl Into<String>, dependencies: Vec<Dependency>) {
        self.edges.insert(package_id.into(), dependencies);
    }

    /// Validate that the graph is acyclic. Returns an error with the cycle
    /// path if a cycle is found.
    pub fn validate_acyclic(&self) -> Result<(), DependencyError> {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();
        let mut path = Vec::new();

        for node in self.edges.keys() {
            if !visited.contains(node) {
                self.dfs_check(node, &mut visited, &mut stack, &mut path)?;
            }
        }
        Ok(())
    }

    fn dfs_check(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Result<(), DependencyError> {
        visited.insert(node.to_string());
        stack.insert(node.to_string());
        path.push(node.to_string());

        if let Some(deps) = self.edges.get(node) {
            for dep in deps {
                let dep_id = &dep.package_id;
                if stack.contains(dep_id) {
                    // Cycle detected.
                    path.push(dep_id.clone());
                    return Err(DependencyError::Circular(path.join(" → ")));
                }
                if !visited.contains(dep_id) {
                    self.dfs_check(dep_id, visited, stack, path)?;
                }
            }
        }

        stack.remove(node);
        path.pop();
        Ok(())
    }

    /// Validate that all declared dependencies exist in the given set of
    /// available packages, and that their versions satisfy the required
    /// ranges.
    ///
    /// `available` is a map: package ID → version.
    pub fn validate_versions(
        &self,
        available: &HashMap<String, crate::version::Version>,
    ) -> Result<(), DependencyError> {
        for (node, deps) in &self.edges {
            for dep in deps {
                let actual = available
                    .get(&dep.package_id)
                    .ok_or_else(|| DependencyError::Missing(dep.package_id.clone()))?;
                if !dep.range.matches(actual) {
                    return Err(DependencyError::VersionMismatch {
                        package_id: dep.package_id.clone(),
                        range: dep.range.to_string(),
                        actual: actual.to_string(),
                    });
                }
                // Avoid unused `node` warning.
                let _ = node;
            }
        }
        Ok(())
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.edges.len()
    }

    /// Returns the topological order of packages (dependency-first). Returns
    /// an error if the graph has cycles.
    pub fn topological_order(&self) -> Result<Vec<String>, DependencyError> {
        self.validate_acyclic()?;
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        for node in self.edges.keys() {
            self.dfs_topo(node, &mut visited, &mut order);
        }
        Ok(order)
    }

    fn dfs_topo(&self, node: &str, visited: &mut HashSet<String>, order: &mut Vec<String>) {
        if visited.contains(node) {
            return;
        }
        visited.insert(node.to_string());
        if let Some(deps) = self.edges.get(node) {
            for dep in deps {
                self.dfs_topo(&dep.package_id, visited, order);
            }
        }
        order.push(node.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::Version;

    fn dep(id: &str, range: &str) -> Dependency {
        Dependency::new(id.into(), range).unwrap()
    }

    fn graph_with_edges(edges: &[(&str, Vec<Dependency>)]) -> DependencyGraph {
        let mut g = DependencyGraph::new();
        for (id, deps) in edges {
            g.add_node(*id, deps.clone());
        }
        g
    }

    #[test]
    fn empty_graph_is_valid() {
        let g = DependencyGraph::new();
        assert!(g.validate_acyclic().is_ok());
    }

    #[test]
    fn linear_dependencies_ok() {
        let g = graph_with_edges(&[
            ("a", vec![dep("b", "^1.0.0")]),
            ("b", vec![dep("c", "^1.0.0")]),
            ("c", vec![]),
        ]);
        assert!(g.validate_acyclic().is_ok());
    }

    #[test]
    fn simple_cycle_detected() {
        let g = graph_with_edges(&[
            ("a", vec![dep("b", "^1.0.0")]),
            ("b", vec![dep("a", "^1.0.0")]),
        ]);
        let err = g.validate_acyclic().unwrap_err();
        assert!(matches!(err, DependencyError::Circular(_)));
    }

    #[test]
    fn three_node_cycle_detected() {
        let g = graph_with_edges(&[
            ("a", vec![dep("b", "^1.0.0")]),
            ("b", vec![dep("c", "^1.0.0")]),
            ("c", vec![dep("a", "^1.0.0")]),
        ]);
        let err = g.validate_acyclic().unwrap_err();
        assert!(matches!(err, DependencyError::Circular(_)));
    }

    #[test]
    fn self_cycle_detected() {
        let g = graph_with_edges(&[("a", vec![dep("a", "^1.0.0")])]);
        let err = g.validate_acyclic().unwrap_err();
        assert!(matches!(err, DependencyError::Circular(_)));
    }

    #[test]
    fn version_validation_missing_dep() {
        let g = graph_with_edges(&[("a", vec![dep("b", "^1.0.0")])]);
        let available = HashMap::new(); // b is missing
        let err = g.validate_versions(&available).unwrap_err();
        assert!(matches!(err, DependencyError::Missing(_)));
    }

    #[test]
    fn version_validation_mismatch() {
        let g = graph_with_edges(&[("a", vec![dep("b", "^1.0.0")])]);
        let mut available = HashMap::new();
        available.insert("b".to_string(), Version::new(2, 0, 0));
        let err = g.validate_versions(&available).unwrap_err();
        assert!(matches!(err, DependencyError::VersionMismatch { .. }));
    }

    #[test]
    fn version_validation_ok() {
        let g = graph_with_edges(&[("a", vec![dep("b", "^1.0.0")])]);
        let mut available = HashMap::new();
        available.insert("b".to_string(), Version::new(1, 5, 3));
        assert!(g.validate_versions(&available).is_ok());
    }

    #[test]
    fn topological_order_works() {
        let g = graph_with_edges(&[
            ("a", vec![dep("b", "^1.0.0"), dep("c", "^1.0.0")]),
            ("b", vec![dep("c", "^1.0.0")]),
            ("c", vec![]),
        ]);
        let order = g.topological_order().unwrap();
        // c must come before b, and b must come before a (or c before a directly).
        let c_idx = order.iter().position(|x| x == "c").unwrap();
        let b_idx = order.iter().position(|x| x == "b").unwrap();
        let a_idx = order.iter().position(|x| x == "a").unwrap();
        assert!(c_idx < b_idx);
        assert!(b_idx < a_idx);
    }

    #[test]
    fn dependency_display() {
        let d = dep("com.nexora.core", "^1.2.0");
        assert_eq!(d.to_string(), "com.nexora.core@^1.2.0");
    }
}

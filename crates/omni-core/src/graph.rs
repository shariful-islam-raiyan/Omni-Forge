//! A small dependency graph with deterministic topological ordering and
//! cycle detection, shared by component resolution and (in the future)
//! build-task scheduling.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub struct DependencyGraph {
    edges: BTreeMap<String, BTreeSet<String>>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node: &str) {
        self.edges.entry(node.to_string()).or_default();
    }

    /// `node` depends on `dependency` (dependency must run/resolve first).
    pub fn add_edge(&mut self, node: &str, dependency: &str) {
        self.add_node(dependency);
        self.edges
            .entry(node.to_string())
            .or_default()
            .insert(dependency.to_string());
    }

    /// Returns a deterministic dependency-first order, or the first cycle
    /// found (as a list of node names forming the cycle) if the graph is not
    /// a DAG.
    pub fn topological_order(&self) -> Result<Vec<String>, Vec<String>> {
        let mut in_progress: BTreeSet<String> = BTreeSet::new();
        let mut done: BTreeSet<String> = BTreeSet::new();
        let mut order: Vec<String> = Vec::new();

        for node in self.edges.keys() {
            if !done.contains(node) {
                let mut stack = Vec::new();
                self.visit(node, &mut in_progress, &mut done, &mut order, &mut stack)?;
            }
        }
        Ok(order)
    }

    fn visit(
        &self,
        node: &str,
        in_progress: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
        order: &mut Vec<String>,
        stack: &mut Vec<String>,
    ) -> Result<(), Vec<String>> {
        if done.contains(node) {
            return Ok(());
        }
        if in_progress.contains(node) {
            let mut cycle = stack.clone();
            cycle.push(node.to_string());
            if let Some(start) = cycle.iter().position(|n| n == node) {
                return Err(cycle[start..].to_vec());
            }
            return Err(cycle);
        }

        in_progress.insert(node.to_string());
        stack.push(node.to_string());

        if let Some(deps) = self.edges.get(node) {
            for dep in deps {
                self.visit(dep, in_progress, done, order, stack)?;
            }
        }

        stack.pop();
        in_progress.remove(node);
        done.insert(node.to_string());
        order.push(node.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_chain_orders_dependencies_first() {
        let mut g = DependencyGraph::new();
        g.add_edge("a", "b");
        g.add_edge("b", "c");
        let order = g.topological_order().unwrap();
        assert_eq!(order, vec!["c", "b", "a"]);
    }

    #[test]
    fn detects_simple_cycle() {
        let mut g = DependencyGraph::new();
        g.add_edge("a", "b");
        g.add_edge("b", "a");
        let err = g.topological_order().unwrap_err();
        assert!(err.contains(&"a".to_string()));
        assert!(err.contains(&"b".to_string()));
    }

    #[test]
    fn deterministic_ordering_for_independent_nodes() {
        let mut g = DependencyGraph::new();
        g.add_node("z");
        g.add_node("a");
        g.add_node("m");
        let order1 = g.topological_order().unwrap();
        let order2 = g.topological_order().unwrap();
        assert_eq!(order1, order2);
    }
}

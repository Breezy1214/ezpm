use serde::Serialize;
use std::collections::HashMap;

// ─── String Interner ─────────────────────────────────────────────────────────

/// Maps file path strings to compact `usize` IDs for O(1) graph operations.
pub struct Interner {
    map: HashMap<String, usize>,
    strings: Vec<String>,
}

impl Interner {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            strings: Vec::new(),
        }
    }

    /// Insert or get an existing ID for the given string.
    pub fn intern(&mut self, s: &str) -> usize {
        if let Some(&id) = self.map.get(s) {
            return id;
        }
        let id = self.strings.len();
        self.strings.push(s.to_string());
        self.map.insert(s.to_string(), id);
        id
    }

    /// Resolve an ID back to its string.
    pub fn resolve(&self, id: usize) -> &str {
        &self.strings[id]
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }
}

// ─── Dependency Graph ────────────────────────────────────────────────────────

/// Adjacency-list dependency graph with interned string IDs.
pub struct DepGraph {
    pub interner: Interner,
    /// node_id → [(target_id, require_path)]
    edges: Vec<Vec<(usize, String)>>,
}

impl DepGraph {
    pub fn new() -> Self {
        Self {
            interner: Interner::new(),
            edges: Vec::new(),
        }
    }

    /// Add a node (file path) to the graph, returning its ID.
    pub fn add_node(&mut self, path: &str) -> usize {
        let id = self.interner.intern(path);
        while self.edges.len() <= id {
            self.edges.push(Vec::new());
        }
        id
    }

    /// Add a directed edge from one node to another with the require path.
    pub fn add_edge(&mut self, from: usize, to: usize, require_path: String) {
        self.edges[from].push((to, require_path));
    }

    /// Get neighbors (outgoing edges) for a node.
    pub fn neighbors(&self, node: usize) -> &[(usize, String)] {
        if node < self.edges.len() {
            &self.edges[node]
        } else {
            &[]
        }
    }

    /// Total number of nodes.
    pub fn node_count(&self) -> usize {
        self.interner.len()
    }

    /// Total number of edges.
    pub fn edge_count(&self) -> usize {
        self.edges.iter().map(|e| e.len()).sum()
    }
}

// ─── Result Types ────────────────────────────────────────────────────────────

/// A circular dependency cycle (list of file paths forming the cycle).
#[derive(Debug, Clone, Serialize)]
pub struct Cycle {
    pub modules: Vec<String>,
}

/// A rule violation (forbidden cross-layer import).
#[derive(Debug, Clone, Serialize)]
pub struct RuleViolation {
    pub from_module: String,
    pub to_module: String,
    pub from_layer: String,
    pub to_layer: String,
    pub reason: Option<String>,
}

/// Aggregated result of all checks.
#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub total_modules: usize,
    pub total_edges: usize,
    pub cycles: Vec<Cycle>,
    pub rule_violations: Vec<RuleViolation>,
    pub unused_modules: Vec<String>,
    pub pass: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interner_dedup() {
        let mut interner = Interner::new();
        let a = interner.intern("src/client/init.luau");
        let b = interner.intern("src/server/init.luau");
        let c = interner.intern("src/client/init.luau");
        assert_eq!(a, c);
        assert_ne!(a, b);
        assert_eq!(interner.len(), 2);
    }

    #[test]
    fn interner_resolve() {
        let mut interner = Interner::new();
        let id = interner.intern("src/shared/util.luau");
        assert_eq!(interner.resolve(id), "src/shared/util.luau");
    }

    #[test]
    fn graph_add_nodes_and_edges() {
        let mut graph = DepGraph::new();
        let a = graph.add_node("a.luau");
        let b = graph.add_node("b.luau");
        graph.add_edge(a, b, "@Shared/b".to_string());

        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 1);
        assert_eq!(graph.neighbors(a).len(), 1);
        assert_eq!(graph.neighbors(a)[0].0, b);
        assert_eq!(graph.neighbors(b).len(), 0);
    }

    #[test]
    fn graph_empty_neighbors_for_unknown_node() {
        let graph = DepGraph::new();
        assert_eq!(graph.neighbors(999).len(), 0);
    }
}

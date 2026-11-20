use serde::Serialize;
use std::collections::HashMap;

#[derive(Default)]
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

    pub fn intern(&mut self, s: &str) -> usize {
        if let Some(&id) = self.map.get(s) {
            return id;
        }
        let id = self.strings.len();
        self.strings.push(s.to_string());
        self.map.insert(s.to_string(), id);
        id
    }

    pub fn get(&self, s: &str) -> Option<usize> {
        self.map.get(s).copied()
    }

    pub fn resolve(&self, id: usize) -> &str {
        &self.strings[id]
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

/// A `require` from one module to another.
#[derive(Debug, Clone)]
pub struct Edge {
    pub to: usize,
    /// The require path as written, such as `@Shared/Signal`.
    pub require: String,
    /// 1-based line of the `require`, when known.
    pub line: Option<usize>,
}

#[derive(Default)]
pub struct DepGraph {
    pub interner: Interner,
    edges: Vec<Vec<Edge>>,
}

impl DepGraph {
    pub fn new() -> Self {
        Self {
            interner: Interner::new(),
            edges: Vec::new(),
        }
    }

    pub fn add_node(&mut self, path: &str) -> usize {
        let id = self.interner.intern(path);
        while self.edges.len() <= id {
            self.edges.push(Vec::new());
        }
        id
    }

    pub fn add_edge(&mut self, from: usize, to: usize, require_path: String) {
        self.add_edge_at(from, to, require_path, None);
    }

    pub fn add_edge_at(&mut self, from: usize, to: usize, require: String, line: Option<usize>) {
        self.edges[from].push(Edge { to, require, line });
    }

    /// The line of the first require from `from` to `to`.
    pub fn edge_line(&self, from: usize, to: usize) -> Option<usize> {
        self.neighbors(from)
            .iter()
            .find(|edge| edge.to == to)
            .and_then(|edge| edge.line)
    }

    pub fn neighbors(&self, node: usize) -> &[Edge] {
        if node < self.edges.len() {
            &self.edges[node]
        } else {
            &[]
        }
    }

    pub fn node_count(&self) -> usize {
        self.interner.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.iter().map(|e| e.len()).sum()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Cycle {
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleViolation {
    pub from_module: String,
    pub to_module: String,
    pub from_layer: String,
    pub to_layer: String,
    pub reason: Option<String>,
    /// Line of the offending require in `from_module`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureViolation {
    pub from_module: String,
    pub to_module: String,
    pub from_feature: String,
    pub to_feature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub total_modules: usize,
    pub total_edges: usize,
    pub cycles: Vec<Cycle>,
    pub rule_violations: Vec<RuleViolation>,
    pub feature_violations: Vec<FeatureViolation>,
    pub unused_modules: Vec<String>,
    pub pass: bool,
}

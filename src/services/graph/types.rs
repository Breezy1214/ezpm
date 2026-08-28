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

#[derive(Default)]
pub struct DepGraph {
    pub interner: Interner,
    edges: Vec<Vec<(usize, String)>>,
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
        self.edges[from].push((to, require_path));
    }

    pub fn neighbors(&self, node: usize) -> &[(usize, String)] {
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
}

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub total_modules: usize,
    pub total_edges: usize,
    pub cycles: Vec<Cycle>,
    pub rule_violations: Vec<RuleViolation>,
    pub unused_modules: Vec<String>,
    pub pass: bool,
}

use super::types::{CheckResult, Cycle, DepGraph, RuleViolation};

pub fn find_cycles(graph: &DepGraph) -> Vec<Cycle> {
    let n = graph.node_count();
    if n == 0 {
        return Vec::new();
    }

    let mut index_counter: usize = 0;
    let mut indices: Vec<Option<usize>> = vec![None; n];
    let mut lowlinks: Vec<usize> = vec![0; n];
    let mut on_stack: Vec<bool> = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut sccs: Vec<Vec<usize>> = Vec::new();

    for start in 0..n {
        if indices[start].is_some() {
            continue;
        }

        let mut call_stack: Vec<(usize, usize)> = vec![(start, 0)];

        indices[start] = Some(index_counter);
        lowlinks[start] = index_counter;
        index_counter += 1;
        on_stack[start] = true;
        stack.push(start);

        while let Some((node, ni)) = call_stack.last_mut() {
            let node = *node;
            let neighbors = graph.neighbors(node);

            if *ni < neighbors.len() {
                let (neighbor, _) = neighbors[*ni];
                *ni += 1;

                if indices[neighbor].is_none() {
                    indices[neighbor] = Some(index_counter);
                    lowlinks[neighbor] = index_counter;
                    index_counter += 1;
                    on_stack[neighbor] = true;
                    stack.push(neighbor);
                    call_stack.push((neighbor, 0));
                } else if on_stack[neighbor] {
                    lowlinks[node] = lowlinks[node].min(indices[neighbor].unwrap());
                }
            } else {
                if lowlinks[node] == indices[node].unwrap() {
                    let mut scc = Vec::new();
                    loop {
                        let w = stack.pop().unwrap();
                        on_stack[w] = false;
                        scc.push(w);
                        if w == node {
                            break;
                        }
                    }
                    sccs.push(scc);
                }

                let (finished_node, _) = call_stack.pop().unwrap();
                if let Some((parent, _)) = call_stack.last() {
                    lowlinks[*parent] = lowlinks[*parent].min(lowlinks[finished_node]);
                }
            }
        }
    }

    sccs.into_iter()
        .filter(|scc| scc.len() > 1)
        .map(|mut scc| {
            scc.reverse();
            Cycle {
                modules: scc
                    .iter()
                    .map(|&id| graph.interner.resolve(id).to_string())
                    .collect(),
            }
        })
        .collect()
}

pub struct ForbidRule {
    pub from: String,
    pub to: String,
    pub reason: Option<String>,
}

pub fn validate_rules(
    graph: &DepGraph,
    layers: &[(String, String)],
    rules: &[ForbidRule],
) -> Vec<RuleViolation> {
    if layers.is_empty() || rules.is_empty() {
        return Vec::new();
    }

    let mut violations = Vec::new();
    let n = graph.node_count();

    for node in 0..n {
        let from_path = graph.interner.resolve(node);
        let from_layer = classify_layer(from_path, layers);

        for (neighbor, _) in graph.neighbors(node) {
            let to_path = graph.interner.resolve(*neighbor);
            let to_layer = classify_layer(to_path, layers);

            if let (Some(ref fl), Some(ref tl)) = (&from_layer, &to_layer) {
                if fl == tl {
                    continue;
                }
                for rule in rules {
                    if rule.from == *fl && rule.to == *tl {
                        violations.push(RuleViolation {
                            from_module: from_path.to_string(),
                            to_module: to_path.to_string(),
                            from_layer: fl.clone(),
                            to_layer: tl.clone(),
                            reason: rule.reason.clone(),
                        });
                    }
                }
            }
        }
    }

    violations
}

fn classify_layer(path: &str, layers: &[(String, String)]) -> Option<String> {
    for (name, prefix) in layers {
        if path.starts_with(prefix.as_str()) {
            return Some(name.clone());
        }
    }
    None
}

pub fn find_unused(graph: &DepGraph, entry_points: &[usize]) -> Vec<String> {
    let n = graph.node_count();
    if n == 0 {
        return Vec::new();
    }

    let mut visited = vec![false; n];
    let mut queue = std::collections::VecDeque::new();

    for &entry in entry_points {
        if entry < n && !visited[entry] {
            visited[entry] = true;
            queue.push_back(entry);
        }
    }

    while let Some(node) = queue.pop_front() {
        for (neighbor, _) in graph.neighbors(node) {
            if !visited[*neighbor] {
                visited[*neighbor] = true;
                queue.push_back(*neighbor);
            }
        }
    }

    (0..n)
        .filter(|&i| !visited[i])
        .map(|i| graph.interner.resolve(i).to_string())
        .collect()
}

pub fn run_all_checks(
    graph: &DepGraph,
    layers: &[(String, String)],
    rules: &[ForbidRule],
    entry_points: &[usize],
) -> CheckResult {
    let cycles = find_cycles(graph);
    let rule_violations = validate_rules(graph, layers, rules);
    let unused_modules = find_unused(graph, entry_points);

    let pass = cycles.is_empty() && rule_violations.is_empty();

    CheckResult {
        total_modules: graph.node_count(),
        total_edges: graph.edge_count(),
        cycles,
        rule_violations,
        unused_modules,
        pass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dag() -> DepGraph {
        let mut g = DepGraph::new();
        let a = g.add_node("a.luau");
        let b = g.add_node("b.luau");
        let c = g.add_node("c.luau");
        g.add_edge(a, b, "b".to_string());
        g.add_edge(b, c, "c".to_string());
        g
    }

    fn make_simple_cycle() -> DepGraph {
        let mut g = DepGraph::new();
        let a = g.add_node("a.luau");
        let b = g.add_node("b.luau");
        g.add_edge(a, b, "b".to_string());
        g.add_edge(b, a, "a".to_string());
        g
    }

    #[test]
    fn no_cycles_in_dag() {
        let g = make_dag();
        let cycles = find_cycles(&g);
        assert!(cycles.is_empty());
    }

    #[test]
    fn detects_simple_cycle() {
        let g = make_simple_cycle();
        let cycles = find_cycles(&g);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].modules.len(), 2);
    }

    #[test]
    fn detects_multiple_cycles() {
        let mut g = DepGraph::new();
        let a = g.add_node("a.luau");
        let b = g.add_node("b.luau");
        let c = g.add_node("c.luau");
        let d = g.add_node("d.luau");
        g.add_edge(a, b, "b".to_string());
        g.add_edge(b, a, "a".to_string());
        g.add_edge(c, d, "d".to_string());
        g.add_edge(d, c, "c".to_string());

        let cycles = find_cycles(&g);
        assert_eq!(cycles.len(), 2);
    }

    #[test]
    fn detects_large_scc() {
        let mut g = DepGraph::new();
        let a = g.add_node("a.luau");
        let b = g.add_node("b.luau");
        let c = g.add_node("c.luau");
        g.add_edge(a, b, "b".to_string());
        g.add_edge(b, c, "c".to_string());
        g.add_edge(c, a, "a".to_string());

        let cycles = find_cycles(&g);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].modules.len(), 3);
    }

    #[test]
    fn rule_validation_detects_forbidden() {
        let mut g = DepGraph::new();
        let client = g.add_node("src/client/ui.luau");
        let server = g.add_node("src/server/handler.luau");
        g.add_edge(client, server, "@Server/handler".to_string());

        let layers = vec![
            ("client".to_string(), "src/client/".to_string()),
            ("server".to_string(), "src/server/".to_string()),
        ];
        let rules = vec![ForbidRule {
            from: "client".to_string(),
            to: "server".to_string(),
            reason: Some("Client must never import server modules".to_string()),
        }];

        let violations = validate_rules(&g, &layers, &rules);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].from_layer, "client");
        assert_eq!(violations[0].to_layer, "server");
    }

    #[test]
    fn rule_validation_allows_permitted() {
        let mut g = DepGraph::new();
        let client = g.add_node("src/client/ui.luau");
        let shared = g.add_node("src/shared/util.luau");
        g.add_edge(client, shared, "@Shared/util".to_string());

        let layers = vec![
            ("client".to_string(), "src/client/".to_string()),
            ("server".to_string(), "src/server/".to_string()),
            ("shared".to_string(), "src/shared/".to_string()),
        ];
        let rules = vec![ForbidRule {
            from: "client".to_string(),
            to: "server".to_string(),
            reason: None,
        }];

        let violations = validate_rules(&g, &layers, &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn find_unused_orphan() {
        let mut g = DepGraph::new();
        let a = g.add_node("src/client/main.luau");
        let b = g.add_node("src/shared/util.luau");
        let orphan = g.add_node("src/shared/orphan.luau");
        g.add_edge(a, b, "@Shared/util".to_string());
        let _ = orphan;

        let unused = find_unused(&g, &[a]);
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0], "src/shared/orphan.luau");
    }

    #[test]
    fn find_unused_transitive_reachability() {
        let mut g = DepGraph::new();
        let a = g.add_node("a.luau");
        let b = g.add_node("b.luau");
        let c = g.add_node("c.luau");
        g.add_edge(a, b, "b".to_string());
        g.add_edge(b, c, "c".to_string());

        let unused = find_unused(&g, &[a]);
        assert!(unused.is_empty());
    }

    #[test]
    fn run_all_checks_clean_project() {
        let g = make_dag();
        let result = run_all_checks(&g, &[], &[], &[0]);
        assert!(result.pass);
        assert!(result.cycles.is_empty());
        assert!(result.rule_violations.is_empty());
    }

    #[test]
    fn run_all_checks_with_cycle() {
        let g = make_simple_cycle();
        let result = run_all_checks(&g, &[], &[], &[0]);
        assert!(!result.pass);
        assert_eq!(result.cycles.len(), 1);
    }
}

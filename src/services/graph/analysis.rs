use std::collections::{HashMap, HashSet, VecDeque};

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
        .filter(|scc| scc.len() > 1 || graph.neighbors(scc[0]).iter().any(|(to, _)| *to == scc[0]))
        .map(|scc| {
            // Tarjan pushes the component root last; start the reported cycle there.
            let start = *scc.last().expect("components are non-empty");
            Cycle {
                modules: shortest_cycle(graph, &scc, start)
                    .iter()
                    .map(|&id| graph.interner.resolve(id).to_string())
                    .collect(),
            }
        })
        .collect()
}

/// Tarjan's stack order is not a path; report a real cycle so every printed arrow
/// in `a -> b -> ... -> a` is an existing edge.
fn shortest_cycle(graph: &DepGraph, scc: &[usize], start: usize) -> Vec<usize> {
    let members: HashSet<usize> = scc.iter().copied().collect();
    let mut parent: HashMap<usize, usize> = HashMap::new();
    let mut queue = VecDeque::from([start]);

    while let Some(node) = queue.pop_front() {
        for &(next, _) in graph.neighbors(node) {
            if next == start {
                let mut path = Vec::new();
                let mut current = node;
                loop {
                    path.push(current);
                    match parent.get(&current) {
                        Some(&prev) => current = prev,
                        None => break,
                    }
                }
                path.reverse();
                return path;
            }
            if members.contains(&next) && !parent.contains_key(&next) {
                parent.insert(next, node);
                queue.push_back(next);
            }
        }
    }

    unreachable!("a strongly connected component always contains a cycle through its root")
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

    fn make_simple_cycle() -> DepGraph {
        let mut g = DepGraph::new();
        let a = g.add_node("a.luau");
        let b = g.add_node("b.luau");
        g.add_edge(a, b, "b".to_string());
        g.add_edge(b, a, "a".to_string());
        g
    }

    #[test]
    fn detects_self_require_cycle() {
        let mut g = DepGraph::new();
        let a = g.add_node("a.luau");
        g.add_node("b.luau");
        g.add_edge(a, a, "a".to_string());

        let cycles = find_cycles(&g);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].modules, vec!["a.luau"]);
    }

    #[test]
    fn reported_cycle_only_uses_existing_edges() {
        let mut g = DepGraph::new();
        let a = g.add_node("a.luau");
        let b = g.add_node("b.luau");
        let c = g.add_node("c.luau");
        g.add_edge(a, b, "b".to_string());
        g.add_edge(b, a, "a".to_string());
        g.add_edge(a, c, "c".to_string());
        g.add_edge(c, a, "a".to_string());

        let cycles = find_cycles(&g);
        assert_eq!(cycles.len(), 1);
        let modules = &cycles[0].modules;
        for (i, from) in modules.iter().enumerate() {
            let to = &modules[(i + 1) % modules.len()];
            let from_id = g.interner.get(from).unwrap();
            let to_id = g.interner.get(to).unwrap();
            assert!(
                g.neighbors(from_id).iter().any(|(t, _)| *t == to_id),
                "{from} -> {to} is not an edge in {modules:?}"
            );
        }
    }

    #[test]
    fn detects_simple_cycle() {
        let g = make_simple_cycle();
        let cycles = find_cycles(&g);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].modules.len(), 2);
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
}

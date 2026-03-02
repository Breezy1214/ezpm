use anyhow::{bail, Result};
use std::path::Path;
use std::time::Instant;

use crate::config::{CheckConfig, EzpmConfig};
use crate::output;
use crate::services::graph::{
    analysis::{self, ForbidRule},
    build,
    types::CheckResult,
};

/// Run dependency analysis: cycle detection, architecture rules, unused module detection.
pub fn run_check(config: Option<&EzpmConfig>, json_output: bool) -> Result<()> {
    let default_config = EzpmConfig::default();
    let cfg = config.unwrap_or(&default_config);

    let aliases = cfg.aliases.clone().unwrap_or_default();
    let src_prefix = cfg
        .paths
        .as_ref()
        .and_then(|p| p.src.as_deref())
        .unwrap_or("src");

    let src_dir = Path::new(src_prefix);
    if !src_dir.exists() {
        bail!(
            "Source directory '{}' not found. Run from your project root or check [paths.src] in ezpm.toml.",
            src_prefix
        );
    }

    let project_root = Path::new(".");

    let spinner = if !json_output {
        Some(output::start_spinner("Building dependency graph..."))
    } else {
        None
    };

    let start = Instant::now();
    let graph = build::build_graph(project_root, &aliases, src_prefix)?;
    let build_time = start.elapsed();

    if let Some(sp) = &spinner {
        sp.finish_and_clear();
    }

    // Extract check config
    let check_config = cfg.check.clone().unwrap_or_default();

    // Build layers list for rule validation
    let layers = build_layers(&check_config, src_prefix);
    let rules = build_rules(&check_config);

    // Determine entry points
    let entry_point_ids = resolve_entry_points(&graph, &check_config, src_prefix);

    let result = analysis::run_all_checks(&graph, &layers, &rules, &entry_point_ids);

    if json_output {
        print_json(&result)?;
    } else {
        print_human(&result, build_time);
    }

    if !result.pass {
        bail!("Dependency check failed");
    }

    Ok(())
}

/// Build the layers mapping from config.
fn build_layers(check_config: &CheckConfig, src_prefix: &str) -> Vec<(String, String)> {
    match &check_config.layers {
        Some(layers) => layers
            .iter()
            .map(|(name, prefix)| (name.clone(), prefix.clone()))
            .collect(),
        None => {
            // Auto-detect standard Roblox layers if source dirs exist
            let mut layers = Vec::new();
            for name in &["client", "server", "shared"] {
                let prefix = format!("{}/{}/", src_prefix, name);
                if Path::new(&format!("{}/{}", src_prefix, name)).exists() {
                    layers.push((name.to_string(), prefix));
                }
            }
            layers
        }
    }
}

/// Convert config ForbidRules to analysis ForbidRules.
fn build_rules(check_config: &CheckConfig) -> Vec<ForbidRule> {
    match &check_config.forbid {
        Some(rules) => rules
            .iter()
            .map(|r| ForbidRule {
                from: r.from.clone(),
                to: r.to.clone(),
                reason: r.reason.clone(),
            })
            .collect(),
        None => Vec::new(),
    }
}

/// Resolve entry points to graph node IDs.
/// Uses configured entry points or auto-detects init files.
fn resolve_entry_points(
    graph: &crate::services::graph::types::DepGraph,
    check_config: &CheckConfig,
    src_prefix: &str,
) -> Vec<usize> {
    let n = graph.node_count();

    if let Some(configured) = &check_config.entry_points {
        return configured
            .iter()
            .filter_map(|path| {
                for i in 0..n {
                    if graph.interner.resolve(i) == *path {
                        return Some(i);
                    }
                }
                output::verbose_line(&format!("Configured entry point not found: {}", path));
                None
            })
            .collect();
    }

    // Auto-detect: any init.luau/init.lua directly under src/<layer>/
    let mut entry_points = Vec::new();
    for i in 0..n {
        let path = graph.interner.resolve(i);
        let is_init = path.ends_with("/init.luau") || path.ends_with("/init.lua");
        if !is_init {
            continue;
        }

        // Check it's a direct child of src/<layer>/ (depth = 2 segments after src/)
        let after_prefix = path.strip_prefix(src_prefix).unwrap_or(path);
        let after_prefix = after_prefix.strip_prefix('/').unwrap_or(after_prefix);
        let segments: Vec<&str> = after_prefix.split('/').collect();
        // e.g. "client/init.luau" → ["client", "init.luau"] (depth 2)
        if segments.len() == 2 {
            entry_points.push(i);
        }
    }

    // If no init files found, treat all nodes as reachable (skip unused detection)
    if entry_points.is_empty() {
        (0..n).collect()
    } else {
        entry_points
    }
}

fn print_json(result: &CheckResult) -> Result<()> {
    let json = serde_json::to_string_pretty(result)
        .map_err(|e| anyhow::anyhow!("Failed to serialize JSON: {}", e))?;
    println!("{}", json);
    Ok(())
}

fn print_human(result: &CheckResult, build_time: std::time::Duration) {
    output::info(&format!(
        "Dependency graph: {} modules, {} dependencies ({:.0}ms)",
        result.total_modules,
        result.total_edges,
        build_time.as_millis()
    ));
    output::print_line("");

    // Cycles
    if result.cycles.is_empty() {
        output::success("No circular dependencies found");
    } else {
        output::error(&format!("Circular dependencies ({})", result.cycles.len()));
        for cycle in &result.cycles {
            let chain = cycle.modules.join(" \u{2192} ");
            // Close the cycle by repeating the first module
            let display = if let Some(first) = cycle.modules.first() {
                format!("  {} \u{2192} {}", chain, first)
            } else {
                format!("  {}", chain)
            };
            output::print_line(&display);
        }
    }
    output::print_line("");

    // Rule violations
    if !result.rule_violations.is_empty() {
        output::error(&format!(
            "Architecture violations ({})",
            result.rule_violations.len()
        ));
        for v in &result.rule_violations {
            output::print_line(&format!("  {} \u{2192} {}", v.from_module, v.to_module));
            let reason_str = v
                .reason
                .as_deref()
                .map(|r| format!(": {}", r))
                .unwrap_or_default();
            output::print_line(&format!(
                "    {} \u{2192} {} is forbidden{}",
                v.from_layer, v.to_layer, reason_str
            ));
        }
        output::print_line("");
    }

    // Unused modules
    if !result.unused_modules.is_empty() {
        output::warn(&format!("Unused modules ({})", result.unused_modules.len()));
        for m in &result.unused_modules {
            output::print_line(&format!("  {}", m));
        }
        output::print_line("");
    }

    // Summary
    if result.pass {
        output::success("All checks passed");
    }
}

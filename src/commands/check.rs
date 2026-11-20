use anyhow::Result;
use std::path::Path;
use std::time::Instant;

use crate::config::{normalize_relative, CheckConfig, EzpmConfig};
use crate::diagnostics::{self, Diagnostic};
use crate::error::EzpmError;
use crate::output;
use crate::services::graph::{
    analysis::{self, FeatureRules, ForbidRule},
    build,
    types::{CheckResult, DepGraph},
};
use crate::services::{
    path_filter::PathFilter,
    rojo_project::RojoProjectSettings,
    sourcemap::{self, SourcemapIndex},
};

pub fn run_check(config: Option<&EzpmConfig>, paths: &[String]) -> Result<()> {
    let default_config = EzpmConfig::default();
    let cfg = config.unwrap_or(&default_config);
    let project_root = std::env::current_dir()?;

    let spinner = output::start_spinner("Building dependency graph...");
    let start = Instant::now();
    let rojo = RojoProjectSettings::from_config(cfg);
    let sourcemap = sourcemap::generate_index(&project_root, &rojo.project)
        .inspect_err(|error| {
            output::verbose_line(&format!("@game requires are not resolved: {error}"));
        })
        .ok();
    let (result, found) = analyze(cfg, &project_root, sourcemap.as_ref(), paths)?;
    let build_time = start.elapsed();
    spinner.finish_and_clear();

    if output::is_json() {
        let mut document = serde_json::to_value(&result)?;
        document["diagnostics"] = serde_json::to_value(&found)?;
        output::document(&document);
    } else {
        print_human(&result, build_time);
        diagnostics::annotate(&found);
    }

    if !result.pass {
        return Err(EzpmError::reported("Dependency check failed", 1).into());
    }

    Ok(())
}

/// Builds the require graph and runs every check, printing nothing. With `paths`, keeps only
/// the findings about those files.
pub fn analyze(
    cfg: &EzpmConfig,
    project_root: &Path,
    sourcemap: Option<&SourcemapIndex>,
    paths: &[String],
) -> Result<(CheckResult, Vec<Diagnostic>)> {
    let aliases = cfg.aliases.clone().unwrap_or_default();
    let src_prefix = cfg.src_dir();
    let src_prefix = src_prefix.as_str();

    if !project_root.join(src_prefix).exists() {
        return Err(EzpmError::project(format!(
            "Source directory '{}' not found. Run from your project root or check [paths.src] in ezpm.toml.",
            src_prefix
        ))
        .into());
    }

    let graph = build::build_graph(project_root, &aliases, src_prefix, sourcemap)?;

    let check_config = cfg.check.clone().unwrap_or_default();
    let layers = build_layers(&check_config, src_prefix);
    let rules = build_rules(&check_config);
    let features = build_feature_rules(&check_config);
    let entry_point_ids = resolve_entry_points(&graph, &check_config, src_prefix);

    let mut result = analysis::run_all_checks(&graph, &layers, &rules, &features, &entry_point_ids);
    let filter = PathFilter::new(project_root, paths);
    if !filter.is_empty() {
        retain_about(&mut result, &filter, project_root);
    }
    let found = diagnostics_for(&result, &graph);
    Ok((result, found))
}

/// Keeps only the findings about the given files: a cycle through one, a violating require
/// from one, or one that is unused.
fn retain_about(result: &mut CheckResult, filter: &PathFilter, project_root: &Path) {
    let about = |module: &str| filter.matches(project_root, Path::new(module));
    result
        .cycles
        .retain(|cycle| cycle.modules.iter().any(|module| about(module)));
    result
        .rule_violations
        .retain(|violation| about(&violation.from_module));
    result
        .feature_violations
        .retain(|violation| about(&violation.from_module));
    result.unused_modules.retain(|module| about(module));
    result.pass = result.passes();
}

fn diagnostics_for(result: &CheckResult, graph: &DepGraph) -> Vec<Diagnostic> {
    let line_between = |from: &str, to: &str| {
        let from = graph.interner.get(from)?;
        let to = graph.interner.get(to)?;
        graph.edge_line(from, to)
    };

    let mut found = Vec::new();
    for cycle in &result.cycles {
        let Some(first) = cycle.modules.first() else {
            continue;
        };
        let next = cycle.modules.get(1).unwrap_or(first);
        let mut chain = cycle.modules.clone();
        chain.push(first.clone());
        found.push(
            Diagnostic::new(
                "check.cycle",
                format!("require cycle: {}", chain.join(" \u{2192} ")),
            )
            .at(first.clone(), line_between(first, next)),
        );
    }
    for violation in &result.rule_violations {
        let reason = violation
            .reason
            .as_deref()
            .map(|r| format!(": {r}"))
            .unwrap_or_default();
        found.push(
            Diagnostic::new(
                "check.forbiddenImport",
                format!(
                    "{} module requires {} module {}{reason}",
                    violation.from_layer, violation.to_layer, violation.to_module
                ),
            )
            .at(violation.from_module.clone(), violation.line),
        );
    }
    for violation in &result.feature_violations {
        found.push(
            Diagnostic::new(
                "check.featureBoundary",
                format!(
                    "feature {} requires feature {}: {}",
                    violation.from_feature, violation.to_feature, violation.to_module
                ),
            )
            .at(violation.from_module.clone(), violation.line),
        );
    }
    for module in &result.unused_modules {
        found.push(
            Diagnostic::new("check.unusedModule", "no entry point requires this module")
                .at(module.clone(), None),
        );
    }
    found
}

fn build_layers(check_config: &CheckConfig, src_prefix: &str) -> Vec<(String, String)> {
    match &check_config.layers {
        Some(layers) => layers
            .iter()
            .map(|(name, prefix)| (name.clone(), prefix.clone()))
            .collect(),
        None => {
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

fn build_feature_rules(check_config: &CheckConfig) -> FeatureRules {
    let as_root = |root: &String| format!("{}/", normalize_relative(root));
    FeatureRules {
        roots: check_config
            .feature_roots
            .iter()
            .flatten()
            .map(as_root)
            .collect(),
        shared: check_config.shared_features.clone().unwrap_or_default(),
    }
}

fn resolve_entry_points(
    graph: &DepGraph,
    check_config: &CheckConfig,
    src_prefix: &str,
) -> Vec<usize> {
    let n = graph.node_count();

    if let Some(configured) = &check_config.entry_points {
        return configured
            .iter()
            .filter_map(|path| {
                let id = graph.interner.get(path);
                if id.is_none() {
                    output::verbose_line(&format!("Configured entry point not found: {}", path));
                }
                id
            })
            .collect();
    }

    let mut entry_points = Vec::new();
    for i in 0..n {
        let path = graph.interner.resolve(i);
        let is_init = path.ends_with("/init.luau") || path.ends_with("/init.lua");
        if !is_init {
            continue;
        }

        let after_prefix = path.strip_prefix(src_prefix).unwrap_or(path);
        let after_prefix = after_prefix.strip_prefix('/').unwrap_or(after_prefix);
        let segments: Vec<&str> = after_prefix.split('/').collect();
        if segments.len() == 2 {
            entry_points.push(i);
        }
    }

    if entry_points.is_empty() {
        (0..n).collect()
    } else {
        entry_points
    }
}

fn print_human(result: &CheckResult, build_time: std::time::Duration) {
    output::info(&format!(
        "Dependency graph: {} modules, {} dependencies ({})",
        result.total_modules,
        result.total_edges,
        output::format_duration(build_time)
    ));
    output::print_line("");

    if result.cycles.is_empty() {
        output::success("No circular dependencies found");
    } else {
        output::error(&format!("Circular dependencies ({})", result.cycles.len()));
        for cycle in &result.cycles {
            let chain = cycle.modules.join(" \u{2192} ");
            let display = if let Some(first) = cycle.modules.first() {
                format!("  {} \u{2192} {}", chain, first)
            } else {
                format!("  {}", chain)
            };
            output::print_line(&display);
        }
    }
    output::print_line("");

    if !result.rule_violations.is_empty() {
        output::error(&format!(
            "Architecture violations ({})",
            result.rule_violations.len()
        ));
        for v in &result.rule_violations {
            let location = diagnostics::location(&v.from_module, v.line);
            output::print_line(&format!("  {} \u{2192} {}", location, v.to_module));
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

    if !result.feature_violations.is_empty() {
        output::error(&format!(
            "Feature boundary violations ({})",
            result.feature_violations.len()
        ));
        for v in &result.feature_violations {
            let location = diagnostics::location(&v.from_module, v.line);
            output::print_line(&format!("  {} \u{2192} {}", location, v.to_module));
            output::print_line(&format!(
                "    feature {} may not require feature {}",
                v.from_feature, v.to_feature
            ));
        }
        output::print_line("");
    }

    if !result.unused_modules.is_empty() {
        output::warn(&format!("Unused modules ({})", result.unused_modules.len()));
        for m in &result.unused_modules {
            output::print_line(&format!("  {}", m));
        }
        output::print_line("");
    }

    if result.pass {
        output::success("All checks passed");
    }
}

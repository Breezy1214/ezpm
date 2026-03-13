use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

use crate::output;
use crate::services::require_fixer::{is_lua_file, require_regex};

use super::types::DepGraph;

/// Build a dependency graph by scanning all Lua files under `project_root/src_prefix`.
///
/// Walks the source directory once, reads each file, parses `require()` calls,
/// resolves paths against aliases, and adds edges to the graph.
///
/// All paths in the graph are relative to `project_root` (e.g. `src/client/init.luau`),
/// matching the format used in aliases and require paths.
pub fn build_graph(
    project_root: &Path,
    aliases: &HashMap<String, String>,
    src_prefix: &str,
) -> Result<DepGraph> {
    let mut graph = DepGraph::new();
    let re = require_regex();
    let scan_dir = project_root.join(src_prefix);

    let mut inverted: Vec<(String, String)> = aliases
        .iter()
        .filter(|(_, path)| {
            path.starts_with(src_prefix)
                || path.starts_with(&format!("{}/", src_prefix))
                || *path == src_prefix
        })
        .map(|(name, path)| {
            let normalized = if path.ends_with('/') {
                path.clone()
            } else {
                format!("{}/", path)
            };
            (normalized, name.clone())
        })
        .collect();
    inverted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    let files: Vec<_> = WalkDir::new(&scan_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_lua_file(e.path()))
        .map(|e| e.into_path())
        .collect();

    for file in &files {
        let rel = normalize_path(file, project_root);
        graph.add_node(&rel);
    }

    for file in &files {
        let rel_from = normalize_path(file, project_root);
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                output::verbose_line(&format!("Could not read {}: {}", file.display(), e));
                continue;
            }
        };

        if !content.contains("require(\"") {
            continue;
        }

        let from_id = graph.add_node(&rel_from);
        for caps in re.captures_iter(&content) {
            let require_path = &caps[1];
            if let Some(resolved) =
                resolve_require(require_path, aliases, src_prefix, project_root, &inverted)
            {
                let to_id = graph.add_node(&resolved);
                graph.add_edge(from_id, to_id, require_path.to_string());
            }
        }
    }

    Ok(graph)
}

/// Resolve a require path to a project-relative file path.
/// Returns None for built-in/external aliases or unresolvable paths.
fn resolve_require(
    require_path: &str,
    aliases: &HashMap<String, String>,
    src_prefix: &str,
    project_root: &Path,
    inverted: &[(String, String)],
) -> Option<String> {
    // Skip built-in Roblox aliases
    if require_path.starts_with("@self") || require_path.starts_with("@game") {
        return None;
    }

    // Skip relative paths
    if require_path.starts_with("./") || require_path.starts_with("../") {
        return None;
    }

    // Try to expand @Alias/path notation
    if require_path.starts_with('@') {
        let without_at = &require_path[1..];
        let (alias_name, remainder) = match without_at.find('/') {
            Some(idx) => (&without_at[..idx], &without_at[idx + 1..]),
            None => (without_at, ""),
        };

        if let Some(real_path) = aliases.get(alias_name) {
            // Check if this alias points outside src/ (external package)
            let is_internal = real_path.starts_with(src_prefix)
                || real_path.starts_with(&format!("{}/", src_prefix))
                || real_path == src_prefix;

            if !is_internal {
                return None;
            }

            let base = if real_path.ends_with('/') {
                format!("{}{}", real_path, remainder)
            } else if remainder.is_empty() {
                real_path.clone()
            } else {
                format!("{}/{}", real_path, remainder)
            };

            return try_resolve_file(&base, project_root);
        }

        output::verbose_line(&format!("Unresolvable alias in require: {}", require_path));
        return None;
    }

    // Bare path (e.g. "src/client/module") — try direct resolution
    if require_path.starts_with(src_prefix) {
        return try_resolve_file(require_path, project_root);
    }

    // Try to match against inverted aliases (e.g. "Client/module" → "src/client/module")
    for (real_prefix, alias_name) in inverted {
        let alias_lower = alias_name.to_lowercase();
        let path_lower = require_path.to_lowercase();
        if path_lower.starts_with(&alias_lower) {
            let remainder = &require_path[alias_name.len()..];
            let remainder = remainder.strip_prefix('/').unwrap_or(remainder);
            let base = format!("{}{}", real_prefix, remainder);
            if let Some(resolved) = try_resolve_file(&base, project_root) {
                return Some(resolved);
            }
        }
    }

    output::verbose_line(&format!("Unresolvable require path: {}", require_path));
    None
}

/// Try to resolve a base path to an actual file using the Luau resolution chain:
/// path.luau → path.lua → path/init.luau → path/init.lua
///
/// `base` is project-relative (e.g. "src/shared/util").
/// `project_root` is the absolute project root for existence checks.
fn try_resolve_file(base: &str, project_root: &Path) -> Option<String> {
    let base = base.trim_end_matches('/');

    // Already has extension?
    if base.ends_with(".luau") || base.ends_with(".lua") {
        if project_root.join(base).exists() {
            return Some(normalize_str(base));
        }
        return None;
    }

    let candidates = [
        format!("{}.luau", base),
        format!("{}.lua", base),
        format!("{}/init.luau", base),
        format!("{}/init.lua", base),
    ];

    for candidate in &candidates {
        if project_root.join(candidate).exists() {
            return Some(normalize_str(candidate));
        }
    }

    None
}

/// Normalize a Path relative to project_root into a forward-slash string.
fn normalize_path(path: &Path, project_root: &Path) -> String {
    let rel = path
        .strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();
    normalize_str(&rel)
}

fn normalize_str(s: &str) -> String {
    s.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project() -> TempDir {
        let dir = TempDir::new().unwrap();
        let p = dir.path();

        fs::create_dir_all(p.join("src/client")).unwrap();
        fs::create_dir_all(p.join("src/server")).unwrap();
        fs::create_dir_all(p.join("src/shared")).unwrap();
        fs::create_dir_all(p.join("Packages")).unwrap();

        fs::write(
            p.join("src/client/init.luau"),
            "local util = require(\"@Shared/util\")\n",
        )
        .unwrap();
        fs::write(p.join("src/shared/util.luau"), "return {}\n").unwrap();
        fs::write(
            p.join("src/server/main.luau"),
            "local util = require(\"@Shared/util\")\n",
        )
        .unwrap();

        dir
    }

    fn test_aliases() -> HashMap<String, String> {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("Server".to_string(), "src/server/".to_string());
        aliases.insert("Shared".to_string(), "src/shared/".to_string());
        aliases.insert("Packages".to_string(), "Packages/".to_string());
        aliases
    }

    #[test]
    fn build_graph_discovers_files() {
        let dir = setup_project();
        let graph = build_graph(dir.path(), &test_aliases(), "src").unwrap();
        assert_eq!(graph.node_count(), 3);
    }

    #[test]
    fn build_graph_creates_edges() {
        let dir = setup_project();
        let graph = build_graph(dir.path(), &test_aliases(), "src").unwrap();
        // client/init.luau → shared/util.luau
        // server/main.luau → shared/util.luau
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn build_graph_skips_external_aliases() {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        fs::create_dir_all(p.join("src/client")).unwrap();
        fs::create_dir_all(p.join("Packages")).unwrap();

        fs::write(
            p.join("src/client/init.luau"),
            "local pkg = require(\"@Packages/something\")\n",
        )
        .unwrap();

        let graph = build_graph(p, &test_aliases(), "src").unwrap();
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn build_graph_skips_builtin_aliases() {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        fs::create_dir_all(p.join("src/client")).unwrap();

        fs::write(
            p.join("src/client/init.luau"),
            "local rs = require(\"@game/ReplicatedStorage\")\nlocal self = require(\"@self/other\")\n",
        )
        .unwrap();

        let graph = build_graph(p, &test_aliases(), "src").unwrap();
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn build_graph_resolves_init_modules() {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        fs::create_dir_all(p.join("src/shared/items")).unwrap();
        fs::create_dir_all(p.join("src/client")).unwrap();

        fs::write(p.join("src/shared/items/init.luau"), "return {}\n").unwrap();
        fs::write(
            p.join("src/client/init.luau"),
            "local items = require(\"@Shared/items\")\n",
        )
        .unwrap();

        let graph = build_graph(p, &test_aliases(), "src").unwrap();
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn build_graph_handles_missing_module_gracefully() {
        let dir = TempDir::new().unwrap();
        let p = dir.path();
        fs::create_dir_all(p.join("src/client")).unwrap();

        fs::write(
            p.join("src/client/init.luau"),
            "local missing = require(\"@Shared/nonexistent\")\n",
        )
        .unwrap();

        let graph = build_graph(p, &test_aliases(), "src").unwrap();
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn build_graph_preserves_edges_on_repeated_build() {
        let dir = setup_project();
        let p = dir.path();
        let aliases = test_aliases();

        let graph = build_graph(p, &aliases, "src").unwrap();
        assert_eq!(graph.edge_count(), 2);

        fs::write(p.join("src/shared/util.luau"), "local x = 1\nreturn x\n").unwrap();

        let graph = build_graph(p, &aliases, "src").unwrap();
        assert_eq!(
            graph.edge_count(),
            2,
            "repeated builds must retain edges from all files"
        );
    }
}

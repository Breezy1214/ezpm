use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use walkdir::WalkDir;

use crate::config::normalize_relative;
use crate::output;
use crate::services::require_fixer::{is_lua_file, require_matches};
use crate::services::rojo_project::is_within;
use crate::services::sourcemap::SourcemapIndex;

use super::types::DepGraph;

pub fn build_graph(
    project_root: &Path,
    aliases: &HashMap<String, String>,
    src_prefix: &str,
    sourcemap: Option<&SourcemapIndex>,
) -> Result<DepGraph> {
    let src_prefix = normalize_relative(src_prefix);
    let mut graph = DepGraph::new();
    let scan_dir = project_root.join(&src_prefix);
    let resolver = Resolver::new(aliases, &src_prefix, project_root, sourcemap);

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

        let from_id = graph.add_node(&rel_from);
        let mut seen_targets = HashSet::new();
        for found in require_matches(&content) {
            if let Some(resolved) = resolver.resolve(found.path) {
                let to_id = graph.add_node(&resolved);
                if seen_targets.insert(to_id) {
                    graph.add_edge(from_id, to_id, found.path.to_string());
                }
            }
        }
    }

    Ok(graph)
}

struct Resolver<'a> {
    aliases: HashMap<&'a str, Option<String>>,
    inverted: Vec<(String, &'a str)>,
    src_prefix: &'a str,
    project_root: &'a Path,
    sourcemap: Option<&'a SourcemapIndex>,
}

impl<'a> Resolver<'a> {
    fn new(
        aliases: &'a HashMap<String, String>,
        src_prefix: &'a str,
        project_root: &'a Path,
        sourcemap: Option<&'a SourcemapIndex>,
    ) -> Self {
        let aliases: HashMap<&str, Option<String>> = aliases
            .iter()
            .map(|(name, path)| {
                let dir = normalize_relative(path);
                (name.as_str(), is_within(&dir, src_prefix).then_some(dir))
            })
            .collect();
        let mut inverted: Vec<(String, &str)> = aliases
            .iter()
            .filter_map(|(name, dir)| Some((format!("{}/", dir.as_ref()?), *name)))
            .collect();
        inverted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        Self {
            aliases,
            inverted,
            src_prefix,
            project_root,
            sourcemap,
        }
    }

    fn resolve(&self, require_path: &str) -> Option<String> {
        if require_path.starts_with("@game/") {
            let source = self.sourcemap?.source_for_game_path(require_path)?;
            let resolved = normalize_path(source, self.project_root);
            return self.is_internal(&resolved).then_some(resolved);
        }

        if require_path.starts_with("@self")
            || require_path == "@game"
            || require_path.starts_with("./")
            || require_path.starts_with("../")
        {
            return None;
        }

        if let Some(without_at) = require_path.strip_prefix('@') {
            let (alias_name, remainder) = without_at.split_once('/').unwrap_or((without_at, ""));
            let Some(dir) = self.aliases.get(alias_name) else {
                output::verbose_line(&format!("Unresolvable alias in require: {}", require_path));
                return None;
            };
            let dir = dir.as_deref()?;
            let base = if remainder.is_empty() {
                dir.to_string()
            } else {
                format!("{}/{}", dir, remainder)
            };
            return try_resolve_file(&base, self.project_root);
        }

        if self.is_internal(require_path) {
            return try_resolve_file(require_path, self.project_root);
        }

        for (real_prefix, alias_name) in &self.inverted {
            if let Some(remainder) = strip_segment_prefix_ignore_case(require_path, alias_name) {
                let base = format!("{}{}", real_prefix, remainder);
                if let Some(resolved) = try_resolve_file(&base, self.project_root) {
                    return Some(resolved);
                }
            }
        }

        output::verbose_line(&format!("Unresolvable require path: {}", require_path));
        None
    }

    fn is_internal(&self, path: &str) -> bool {
        path != self.src_prefix && is_within(path, self.src_prefix)
    }
}

fn strip_segment_prefix_ignore_case<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    let head = path.get(..prefix.len())?;
    if !head.eq_ignore_ascii_case(prefix) {
        return None;
    }
    let rest = &path[prefix.len()..];
    if rest.is_empty() {
        Some(rest)
    } else {
        rest.strip_prefix('/')
    }
}

fn try_resolve_file(base: &str, project_root: &Path) -> Option<String> {
    let base = base.trim_end_matches('/');

    if base.ends_with(".luau") || base.ends_with(".lua") {
        if project_root.join(base).is_file() {
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
        if project_root.join(candidate).is_file() {
            return Some(normalize_str(candidate));
        }
    }

    None
}

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
    fn build_graph_creates_edges() {
        let dir = setup_project();
        let graph = build_graph(dir.path(), &test_aliases(), "src", None).unwrap();
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

        let graph = build_graph(p, &test_aliases(), "src", None).unwrap();
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

        let graph = build_graph(p, &test_aliases(), "src", None).unwrap();
        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn build_graph_preserves_edges_on_repeated_build() {
        let dir = setup_project();
        let p = dir.path();
        let aliases = test_aliases();

        let graph = build_graph(p, &aliases, "src", None).unwrap();
        assert_eq!(graph.edge_count(), 2);

        fs::write(p.join("src/shared/util.luau"), "local x = 1\nreturn x\n").unwrap();

        let graph = build_graph(p, &aliases, "src", None).unwrap();
        assert_eq!(
            graph.edge_count(),
            2,
            "repeated builds must retain edges from all files"
        );
    }

    #[test]
    fn build_graph_resolves_game_paths_through_the_sourcemap() {
        let dir = setup_project();
        let p = dir.path();
        fs::write(
            p.join("src/client/init.luau"),
            "local util = require(\"@game/ReplicatedStorage/Shared/util\")\n",
        )
        .unwrap();
        let sourcemap = SourcemapIndex::from_pairs(&[(
            p.join("src/shared/util.luau"),
            "@game/ReplicatedStorage/Shared/util",
        )]);

        let graph = build_graph(p, &test_aliases(), "src", Some(&sourcemap)).unwrap();
        let client = graph.interner.get("src/client/init.luau").unwrap();
        let util = graph.interner.get("src/shared/util.luau").unwrap();

        assert!(graph.neighbors(client).iter().any(|(to, _)| *to == util));
    }

    #[test]
    fn build_graph_ignores_comments_and_duplicate_requires() {
        let dir = setup_project();
        let p = dir.path();
        fs::write(
            p.join("src/client/init.luau"),
            "-- local old = require(\"@Shared/util\")\nlocal a = require(\"@Shared/util\")\nlocal b = require(\"@Shared/util\")\n",
        )
        .unwrap();
        fs::write(p.join("src/server/main.luau"), "return nil\n").unwrap();

        let graph = build_graph(p, &test_aliases(), "src", None).unwrap();

        assert_eq!(graph.edge_count(), 1);
    }

    #[test]
    fn build_graph_normalizes_src_prefix_and_respects_segment_boundaries() {
        let dir = setup_project();
        let p = dir.path();
        fs::create_dir_all(p.join("src/shared/Utils")).unwrap();
        fs::write(p.join("src/shared/Utils/x.luau"), "return {}\n").unwrap();
        fs::write(
            p.join("src/server/main.luau"),
            "local a = require(\"SharedUtils/x\")\nlocal b = require(\"\u{212A}/x\")\n",
        )
        .unwrap();

        let graph = build_graph(p, &test_aliases(), "./src/", None).unwrap();

        assert_eq!(
            graph.edge_count(),
            1,
            "only the client's @Shared/util edge remains"
        );
    }
}

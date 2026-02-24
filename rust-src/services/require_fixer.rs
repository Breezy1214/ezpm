use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// ─── Public types ────────────────────────────────────────────────────────────

/// Overall result of running fix_requires over a directory tree.
#[derive(Debug, Default)]
pub struct FixResult {
    /// Number of files that had at least one require rewritten.
    pub files_changed: usize,
    /// Total number of .lua/.luau files that were scanned.
    pub total_files_scanned: usize,
    /// Per-file change details for display.
    pub changes: Vec<FileChange>,
}

/// Per-file change information.
#[derive(Debug)]
pub struct FileChange {
    /// Path to the file that was changed.
    pub file: PathBuf,
    /// Each individual require rewrite performed in this file.
    pub rewrites: Vec<RequireRewrite>,
}

/// A single require path rewrite.
#[derive(Debug, Clone, PartialEq)]
pub struct RequireRewrite {
    /// The old require path (before rewriting).
    pub old: String,
    /// The new require path (after rewriting).
    pub new: String,
}

// ─── Public function stubs (RED phase — implementation pending) ───────────────

/// Scan all .lua/.luau files under `root_dir` and rewrite require paths to
/// `@alias` notation using the provided alias map.
///
/// Only writes files to disk when changes are actually made (BUILD-04).
/// Returns a FixResult with per-file change details for display (BUILD-05).
///
/// `src_prefix` is the source directory prefix (e.g. `"src"`) used to
/// distinguish internal aliases (rooted under src/) from external ones.
pub fn fix_requires(
    _root_dir: &Path,
    _aliases: &HashMap<String, String>,
    _src_prefix: &str,
) -> Result<FixResult> {
    todo!("fix_requires not yet implemented")
}

/// Process a single file, rewriting its require paths in place.
///
/// Returns `Some(FileChange)` if changes were made, `None` otherwise.
/// Only writes the file to disk when changes are actually made (BUILD-04).
pub fn fix_single_file(
    _file_path: &Path,
    _aliases: &HashMap<String, String>,
    _src_prefix: &str,
) -> Result<Option<FileChange>> {
    todo!("fix_single_file not yet implemented")
}

// ─── Internal function stubs ──────────────────────────────────────────────────

/// Build the list of src-rooted aliases sorted by real path length descending.
///
/// Only includes aliases whose path starts with `{src_prefix}/`.
/// Maps each to `("@{name}/", path)` tuples.
/// Ties (same path length) are broken alphabetically by alias name for
/// deterministic behaviour — matching Luau `getSortedSrcShortcuts`.
fn build_sorted_src_aliases(
    _aliases: &HashMap<String, String>,
    _src_prefix: &str,
) -> Vec<(String, String)> {
    vec![] // stub — returns empty, causing tests to fail
}

/// Build the list of require path prefixes that should be left untouched.
///
/// Always includes @self and @game (with and without trailing slash).
/// Also includes `@{name}/` for every alias whose path does NOT start with
/// `{src_prefix}/` (i.e. external aliases like Packages/, ServerPackages/).
fn build_skip_list(_aliases: &HashMap<String, String>, _src_prefix: &str) -> Vec<String> {
    vec![] // stub — returns empty, causing tests to fail
}

/// Check whether a file path is a Lua or Luau source file.
pub fn is_lua_file(_path: &Path) -> bool {
    false // stub — always returns false, causing tests to fail
}

/// Return the compiled require-path regex (compiled once, reused across calls).
///
/// Pattern: `require("...")` — double-quotes only, matching Luau `require%("(.-)"%)`.
/// Single-quote requires are intentionally NOT matched (Pitfall 2 / Luau parity).
pub fn require_regex() -> &'static Regex {
    static REQUIRE_RE: OnceLock<Regex> = OnceLock::new();
    REQUIRE_RE.get_or_init(|| {
        Regex::new(r#"require\("([^"]+)"\)"#).expect("require regex is valid")
    })
}

/// Pure transformation: scan `content` for `require("...")` calls and rewrite
/// any that match src aliases.
///
/// Returns `(new_content, rewrites)`.  If no rewrites were needed, `rewrites`
/// is empty and `new_content` equals `content` unchanged.
///
/// This is a pure function with no filesystem I/O — safe to call from tests
/// without a temporary directory.
pub fn process_file_content(
    content: &str,
    _sorted_aliases: &[(String, String)],
    _skip_list: &[String],
    _src_prefix: &str,
) -> (String, Vec<RequireRewrite>) {
    (content.to_string(), vec![]) // stub — no rewrites, causing tests to fail
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    // ── Helper: standard alias map used across many tests ────────────────────

    fn standard_aliases() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("Client".to_string(), "src/client/".to_string());
        m.insert("Server".to_string(), "src/server/".to_string());
        m.insert("Shared".to_string(), "src/shared/".to_string());
        m.insert("Packages".to_string(), "Packages/".to_string());
        m.insert("ServerPackages".to_string(), "ServerPackages/".to_string());
        m
    }

    // ── Alias sorting tests ───────────────────────────────────────────────────

    #[test]
    fn test_longest_match_wins() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("SharedClient".to_string(), "src/client/shared/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");

        // Must have at least 2 entries
        assert!(sorted.len() >= 2, "expected 2 sorted aliases, got {:?}", sorted);
        // SharedClient's real path (src/client/shared/) is longer → must appear first
        assert!(
            sorted[0].1.len() >= sorted[1].1.len(),
            "longer path must sort first: {:?}",
            sorted
        );
        // Verify SharedClient is actually first
        assert_eq!(
            sorted[0].0, "@SharedClient/",
            "SharedClient should be first due to longer path"
        );
    }

    #[test]
    fn test_sort_stability_on_equal_length() {
        // Two aliases with paths of equal length — sort must be deterministic (alphabetical by name)
        let mut aliases = HashMap::new();
        aliases.insert("Alpha".to_string(), "src/alpha/".to_string()); // len 10
        aliases.insert("Beta".to_string(), "src/beta_/".to_string()); // len 10

        let sorted = build_sorted_src_aliases(&aliases, "src");

        assert_eq!(sorted.len(), 2, "both aliases should appear");
        // Equal length → alphabetical by alias name (@Alpha < @Beta)
        assert_eq!(sorted[0].0, "@Alpha/", "Alpha should sort before Beta");
        assert_eq!(sorted[1].0, "@Beta/", "Beta should sort after Alpha");
    }

    #[test]
    fn test_only_src_aliases_in_sorted_list() {
        let aliases = standard_aliases();
        let sorted = build_sorted_src_aliases(&aliases, "src");

        // Packages and ServerPackages are external (not under src/) → must NOT appear
        let names: Vec<&str> = sorted.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            !names.contains(&"@Packages/"),
            "Packages must not appear in src aliases"
        );
        assert!(
            !names.contains(&"@ServerPackages/"),
            "ServerPackages must not appear in src aliases"
        );
        // src aliases (Client, Server, Shared) must appear
        assert!(names.contains(&"@Client/"), "Client must appear");
        assert!(names.contains(&"@Server/"), "Server must appear");
        assert!(names.contains(&"@Shared/"), "Shared must appear");
    }

    // ── Skip list tests ───────────────────────────────────────────────────────

    #[test]
    fn test_external_aliases_in_skip_list() {
        let aliases = standard_aliases();
        let skip = build_skip_list(&aliases, "src");

        assert!(
            skip.contains(&"@Packages/".to_string()),
            "Packages must be in skip list"
        );
        assert!(
            skip.contains(&"@ServerPackages/".to_string()),
            "ServerPackages must be in skip list"
        );
    }

    #[test]
    fn test_src_aliases_not_in_skip_list() {
        let aliases = standard_aliases();
        let skip = build_skip_list(&aliases, "src");

        assert!(
            !skip.contains(&"@Client/".to_string()),
            "Client (src-rooted) must not be in skip list"
        );
        assert!(
            !skip.contains(&"@Server/".to_string()),
            "Server (src-rooted) must not be in skip list"
        );
        assert!(
            !skip.contains(&"@Shared/".to_string()),
            "Shared (src-rooted) must not be in skip list"
        );
    }

    #[test]
    fn test_builtin_aliases_always_skipped() {
        let aliases: HashMap<String, String> = HashMap::new();
        let skip = build_skip_list(&aliases, "src");

        assert!(skip.contains(&"@self/".to_string()), "@self/ must be in skip list");
        assert!(skip.contains(&"@self".to_string()), "@self must be in skip list");
        assert!(skip.contains(&"@game/".to_string()), "@game/ must be in skip list");
        assert!(skip.contains(&"@game".to_string()), "@game must be in skip list");
    }

    // ── Require detection tests ───────────────────────────────────────────────

    #[test]
    fn test_finds_double_quote_requires() {
        let content = r#"local m = require("@Client/module")"#;
        let re = require_regex();
        let caps: Vec<&str> = re
            .captures_iter(content)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        assert_eq!(caps, vec!["@Client/module"], "double-quote require must be found");
    }

    #[test]
    fn test_ignores_single_quote_requires() {
        let content = "local m = require('@Client/module')";
        let re = require_regex();
        let caps: Vec<&str> = re
            .captures_iter(content)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        assert!(caps.is_empty(), "single-quote require must NOT be matched (Luau parity)");
    }

    #[test]
    fn test_finds_multiple_requires_per_file() {
        let content = r#"
local a = require("src/client/a")
local b = require("src/server/b")
local c = require("@Packages/rodux")
"#;
        let re = require_regex();
        let caps: Vec<&str> = re
            .captures_iter(content)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        assert_eq!(caps.len(), 3, "all 3 double-quote requires must be found");
    }

    // ── Content rewriting tests (pure function, no filesystem) ───────────────

    #[test]
    fn test_rewrites_src_path_to_alias() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        let content = r#"local m = require("src/client/module")"#;
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src");

        assert_eq!(rewrites.len(), 1, "one rewrite should occur");
        assert_eq!(rewrites[0].old, "src/client/module");
        assert_eq!(rewrites[0].new, "@Client/module");
        assert!(
            new_content.contains(r#"require("@Client/module")"#),
            "content must contain rewritten require"
        );
    }

    #[test]
    fn test_longest_match_applied() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("SharedClient".to_string(), "src/client/shared/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        let content = r#"local m = require("src/client/shared/util")"#;
        let (_, rewrites) = process_file_content(content, &sorted, &skip, "src");

        assert_eq!(rewrites.len(), 1);
        // SharedClient is the longer (more specific) match
        assert_eq!(rewrites[0].new, "@SharedClient/util", "SharedClient must win over Client");
    }

    #[test]
    fn test_skipped_aliases_untouched() {
        let aliases = standard_aliases();
        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        let content = r#"local r = require("@Packages/rodux")"#;
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src");

        assert!(rewrites.is_empty(), "external alias must not be rewritten");
        assert_eq!(new_content, content, "content must be unchanged");
    }

    #[test]
    fn test_builtin_self_untouched() {
        let sorted: Vec<(String, String)> = vec![];
        let skip = build_skip_list(&HashMap::new(), "src");

        let content = r#"local m = require("@self/module")"#;
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src");

        assert!(rewrites.is_empty(), "@self require must not be rewritten");
        assert_eq!(new_content, content, "content must be unchanged");
    }

    #[test]
    fn test_builtin_game_untouched() {
        let sorted: Vec<(String, String)> = vec![];
        let skip = build_skip_list(&HashMap::new(), "src");

        let content = r#"local m = require("@game/ReplicatedStorage")"#;
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src");

        assert!(rewrites.is_empty(), "@game require must not be rewritten");
        assert_eq!(new_content, content, "content must be unchanged");
    }

    #[test]
    fn test_no_change_returns_empty_rewrites() {
        let aliases = standard_aliases();
        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        // Content has no requires at all
        let content = "local x = 42\nreturn x\n";
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src");

        assert!(rewrites.is_empty(), "no rewrites for content with no requires");
        assert_eq!(new_content, content, "content must be unchanged");
    }

    #[test]
    fn test_multiple_rewrites_in_one_file() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("Server".to_string(), "src/server/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        let content = r#"
local a = require("src/client/ui")
local b = require("src/server/api")
"#;
        let (_, rewrites) = process_file_content(content, &sorted, &skip, "src");

        assert_eq!(rewrites.len(), 2, "both requires must be rewritten");
        let new_paths: Vec<&str> = rewrites.iter().map(|r| r.new.as_str()).collect();
        assert!(new_paths.contains(&"@Client/ui"), "Client rewrite must be present");
        assert!(new_paths.contains(&"@Server/api"), "Server rewrite must be present");
    }

    #[test]
    fn test_already_aliased_path_untouched() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        // Path already uses @Client/ notation
        let content = r#"local m = require("@Client/module")"#;
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src");

        assert!(rewrites.is_empty(), "already-aliased path must not be rewritten");
        assert_eq!(new_content, content, "content must be unchanged");
    }

    // ── Filesystem integration tests ──────────────────────────────────────────

    fn make_temp_tree(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().expect("failed to create temp dir");
        for (path, content) in files {
            let full = dir.path().join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("failed to create dir");
            }
            std::fs::write(&full, content).expect("failed to write file");
        }
        dir
    }

    #[test]
    fn test_fix_requires_scans_recursively() {
        let dir = make_temp_tree(&[
            ("a.luau", r#"local x = require("src/client/a")"#),
            ("nested/b.luau", r#"local x = require("src/client/b")"#),
            ("nested/deep/c.luau", r#"local x = require("src/client/c")"#),
        ]);

        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());

        let result = fix_requires(dir.path(), &aliases, "src").expect("fix_requires must succeed");

        assert_eq!(result.total_files_scanned, 3, "all 3 .luau files must be scanned");
        assert_eq!(result.files_changed, 3, "all 3 files must be changed");
    }

    #[test]
    fn test_only_lua_files_scanned() {
        let dir = make_temp_tree(&[
            ("a.luau", r#"local x = require("src/client/a")"#),
            ("b.lua", r#"local x = require("src/client/b")"#),
            ("c.txt", r#"require("src/client/c")"#),
            ("d.json", r#"{"key": "value"}"#),
        ]);

        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());

        let result = fix_requires(dir.path(), &aliases, "src").expect("fix_requires must succeed");

        // Only .luau and .lua files are scanned
        assert_eq!(result.total_files_scanned, 2, ".txt and .json must not be scanned");
    }

    #[test]
    fn test_unchanged_files_not_written() {
        let dir = make_temp_tree(&[("no_requires.luau", "local x = 42\nreturn x\n")]);

        let file_path = dir.path().join("no_requires.luau");
        let mtime_before = std::fs::metadata(&file_path)
            .expect("metadata")
            .modified()
            .expect("mtime");

        // Small sleep to ensure mtime difference would be observable
        std::thread::sleep(std::time::Duration::from_millis(10));

        let aliases = standard_aliases();
        fix_requires(dir.path(), &aliases, "src").expect("fix_requires must succeed");

        let mtime_after = std::fs::metadata(&file_path)
            .expect("metadata")
            .modified()
            .expect("mtime");

        assert_eq!(
            mtime_before, mtime_after,
            "unchanged file must not be written to disk"
        );
    }

    #[test]
    fn test_changed_files_written_to_disk() {
        let dir = make_temp_tree(&[(
            "has_require.luau",
            r#"local m = require("src/client/module")"#,
        )]);

        let file_path = dir.path().join("has_require.luau");

        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());

        fix_requires(dir.path(), &aliases, "src").expect("fix_requires must succeed");

        let updated = std::fs::read_to_string(&file_path).expect("read updated file");
        assert!(
            updated.contains(r#"require("@Client/module")"#),
            "file content must be updated on disk: {updated}"
        );
    }

    #[test]
    fn test_fix_result_counts_correct() {
        let dir = make_temp_tree(&[
            ("a.luau", r#"local x = require("src/client/a")"#),
            ("b.luau", "local y = 42\n"), // no requires
            ("c.luau", r#"local z = require("src/server/z")"#),
        ]);

        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("Server".to_string(), "src/server/".to_string());

        let result = fix_requires(dir.path(), &aliases, "src").expect("fix_requires must succeed");

        assert_eq!(result.total_files_scanned, 3, "3 files scanned");
        assert_eq!(result.files_changed, 2, "only 2 files changed (b.luau has no requires)");
        assert_eq!(result.changes.len(), 2, "2 FileChange entries");
    }
}

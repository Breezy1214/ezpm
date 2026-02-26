use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use walkdir::WalkDir;

use crate::output;

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

// ─── Public functions ─────────────────────────────────────────────────────────

/// Scan all .lua/.luau files under `root_dir` and rewrite require paths to
/// `@alias` notation using the provided alias map.
///
/// Only writes files to disk when changes are actually made (BUILD-04).
/// Returns a FixResult with per-file change details for display (BUILD-05).
///
/// `src_prefix` is the source directory prefix (e.g. `"src"`) used to
/// distinguish internal aliases (rooted under src/) from external ones.
pub fn fix_requires(
    root_dir: &Path,
    aliases: &HashMap<String, String>,
    src_prefix: &str,
) -> Result<FixResult> {
    // Build sorted alias list, skip list, and inverted aliases once (performance)
    let sorted_aliases = build_sorted_src_aliases(aliases, src_prefix);
    let skip_list = build_skip_list(aliases, src_prefix);
    let inverted_aliases = build_inverted_aliases(aliases);

    let mut result = FixResult::default();

    for entry in WalkDir::new(root_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_lua_file(e.path()))
    {
        result.total_files_scanned += 1;

        let content = std::fs::read_to_string(entry.path())?;
        let (new_content, rewrites) = process_file_content(
            &content,
            &sorted_aliases,
            &skip_list,
            src_prefix,
            Some(root_dir),
            &inverted_aliases,
        );

        if !rewrites.is_empty() {
            // BUILD-04: only write when changes are actually made
            std::fs::write(entry.path(), &new_content)?;
            result.files_changed += 1;
            result.changes.push(FileChange {
                file: entry.path().to_path_buf(),
                rewrites,
            });
        }
    }

    Ok(result)
}

/// Process a single file, rewriting its require paths in place.
pub fn fix_single_file(
    file_path: &Path,
    aliases: &HashMap<String, String>,
    src_prefix: &str,
) -> Result<Option<FileChange>> {
    let sorted_aliases = build_sorted_src_aliases(aliases, src_prefix);
    let skip_list = build_skip_list(aliases, src_prefix);
    let inverted_aliases = build_inverted_aliases(aliases);

    let content = std::fs::read_to_string(file_path)?;
    let (new_content, rewrites) = process_file_content(
        &content,
        &sorted_aliases,
        &skip_list,
        src_prefix,
        Some(Path::new(src_prefix)),
        &inverted_aliases,
    );

    if rewrites.is_empty() {
        return Ok(None);
    }

    // BUILD-04: only write when changes are actually made
    std::fs::write(file_path, &new_content)?;
    Ok(Some(FileChange {
        file: file_path.to_path_buf(),
        rewrites,
    }))
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Normalize path separators to forward slashes (Windows compat).
/// Alias paths use `/` but PathBuf::to_string_lossy() produces `\` on Windows.
fn normalize_separators(s: &str) -> String {
    s.replace('\\', "/")
}
fn build_sorted_src_aliases(
    aliases: &HashMap<String, String>,
    src_prefix: &str,
) -> Vec<(String, String)> {
    let prefix = format!("{src_prefix}/");
    let mut list: Vec<(String, String)> = aliases
        .iter()
        .filter(|(_, path)| path.starts_with(&prefix) || *path == src_prefix)
        .map(|(name, path)| {
            let real_path = if path.ends_with('/') {
                path.clone()
            } else {
                format!("{path}/")
            };
            (format!("@{name}/"), real_path)
        })
        .collect();

    // Sort by real path length descending; break ties by alias shortcut name
    // ascending (alphabetical) for deterministic behaviour (Pitfall 6).
    list.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    list
}

/// Build the list of require path prefixes that should be left untouched.
fn build_skip_list(aliases: &HashMap<String, String>, src_prefix: &str) -> Vec<String> {
    let prefix = format!("{src_prefix}/");
    let mut skip = vec![
        "@self/".to_string(),
        "@self".to_string(),
        "@game/".to_string(),
        "@game".to_string(),
    ];
    for (name, path) in aliases {
        if !path.starts_with(&prefix) && path != src_prefix {
            skip.push(format!("@{name}/"));
        }
    }
    skip
}

/// Check whether a file path is a Lua or Luau source file.
pub fn is_lua_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("lua") | Some("luau")
    )
}

/// Return the compiled require-path regex
pub fn require_regex() -> &'static Regex {
    static REQUIRE_RE: OnceLock<Regex> = OnceLock::new();
    REQUIRE_RE.get_or_init(|| {
        Regex::new(r#"require\("([^"]+)"\)"#).expect("require regex is valid")
    })
}

/// Recursively search for a file by name under `search_dir`.
/// Checks exact name, name.luau, and name.lua (matching Luau's findFileByName).
fn find_file_by_name(expected_name: &str, search_dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(search_dir).ok()?;
    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let path = entry.path();

        if name_str == expected_name
            || name_str == format!("{expected_name}.luau")
            || name_str == format!("{expected_name}.lua")
        {
            if path.is_dir() {
                return Some(path.join("init.luau"));
            }
            return Some(path);
        }

        if path.is_dir() {
            if let Some(found) = find_file_by_name(expected_name, &path) {
                return Some(found);
            }
        }
    }

    None
}

/// Build inverted aliases sorted by real path length descending.
/// Maps `"src/client/" → "@Client/"` for converting found file paths back to alias notation.
pub fn build_inverted_aliases(aliases: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut inverted: Vec<(String, String)> = aliases
        .iter()
        .map(|(name, path)| {
            let real_path = if path.ends_with('/') {
                path.clone()
            } else {
                format!("{path}/")
            };
            (real_path, format!("@{name}/"))
        })
        .collect();

    // Sort by real path length descending (longest match first)
    inverted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    inverted
}

/// Convert a found file path to alias notation using inverted aliases.
/// Strips `/init.luau`, `/init.lua`, `.luau`, `.lua` suffixes (matching Luau behavior).
fn convert_path_to_alias(file_path: &str, inverted_aliases: &[(String, String)]) -> String {
    let mut converted = normalize_separators(file_path);
    for (real_path, shortcut) in inverted_aliases {
        if converted.contains(real_path.as_str()) {
            converted = converted.replace(real_path.as_str(), shortcut.as_str());
            break;
        }
    }

    // Strip init file patterns first, then plain extensions
    if converted.ends_with("/init.luau") {
        converted.truncate(converted.len() - "/init.luau".len());
    } else if converted.ends_with("/init.lua") {
        converted.truncate(converted.len() - "/init.lua".len());
    } else if converted.ends_with("/init") {
        converted.truncate(converted.len() - "/init".len());
    } else if let Some(stripped) = converted.strip_suffix(".luau") {
        converted = stripped.to_string();
    } else if let Some(stripped) = converted.strip_suffix(".lua") {
        converted = stripped.to_string();
    }

    converted
}

fn starts_with_ignore_ascii_case(s: &str, prefix: &str) -> bool {
    s.get(..prefix.len())
        .map(|head| head.eq_ignore_ascii_case(prefix))
        .unwrap_or(false)
}

fn is_builtin_alias_path(required_path: &str) -> bool {
    starts_with_ignore_ascii_case(required_path, "@self/")
        || required_path.eq_ignore_ascii_case("@self")
        || starts_with_ignore_ascii_case(required_path, "@game/")
        || required_path.eq_ignore_ascii_case("@game")
}

fn rewrite_require_path(
    required_path: &str,
    sorted_aliases: &[(String, String)],
    skip_list: &[String],
    src_prefix: &str,
    root_dir: Option<&Path>,
    inverted_aliases: &[(String, String)],
) -> Option<String> {
    if skip_list
        .iter()
        .any(|entry| required_path.starts_with(entry.as_str()))
    {
        return None;
    }

    if is_builtin_alias_path(required_path) {
        return None;
    }

    if required_path.starts_with("../") || required_path.starts_with("./") {
        output::warn(&format!(
            "relative require path '{}' is not supported — leaving untouched",
            required_path
        ));
        return None;
    }

    for (alias_shortcut, real_path) in sorted_aliases {
        if required_path.starts_with(real_path.as_str()) {
            return Some(format!(
                "{}{}",
                alias_shortcut,
                &required_path[real_path.len()..]
            ));
        }
    }

    let src_slash = format!("{src_prefix}/");
    if required_path.starts_with(&src_slash) || required_path == src_prefix {
        if let Some(root) = root_dir {
            if let Some(file_name) = required_path.rsplit('/').next() {
                if let Some(found) = find_file_by_name(file_name, root) {
                    let found_str = normalize_separators(&found.to_string_lossy());
                    let converted = convert_path_to_alias(&found_str, inverted_aliases);
                    if converted != required_path {
                        return Some(converted);
                    }
                } else {
                    output::warn(&format!(
                        "could not find file for source path '{}'",
                        required_path
                    ));
                }
            }
        } else {
            output::warn(&format!(
                "unresolved src require path '{}' — no alias matches, leaving untouched",
                required_path
            ));
        }
        return None;
    }

    let mut resolved_path = required_path.to_string();
    for (shortcut, real_path) in sorted_aliases {
        if required_path.starts_with(shortcut.as_str()) {
            resolved_path = format!("{}{}", real_path, &required_path[shortcut.len()..]);
            break;
        }
    }

    if resolved_path.contains("../") || resolved_path.contains("./") {
        output::warn(&format!(
            "relative require path '{}' (resolved: '{}') is not supported — leaving untouched",
            required_path, resolved_path
        ));
        return None;
    }

    if let Some(root) = root_dir {
        let as_file_path = resolved_path.replace('.', "/");
        let as_file_path = if as_file_path.ends_with("/luau") {
            &as_file_path[..as_file_path.len() - "/luau".len()]
        } else if as_file_path.ends_with("/lua") {
            &as_file_path[..as_file_path.len() - "/lua".len()]
        } else {
            &as_file_path
        };

        let full_path = format!("{as_file_path}.luau");
        let init_path = format!("{as_file_path}/init.luau");

        if !Path::new(&full_path).is_file() && !Path::new(&init_path).is_file() {
            if let Some(file_name) = as_file_path.rsplit('/').next() {
                if let Some(found) = find_file_by_name(file_name, root) {
                    let found_str = normalize_separators(&found.to_string_lossy());
                    let converted = convert_path_to_alias(&found_str, inverted_aliases);
                    if converted != required_path {
                        return Some(converted);
                    }
                } else {
                    output::warn(&format!(
                        "could not find file '{}' for require path '{}'",
                        file_name, required_path
                    ));
                }
            }
        }
    }

    None
}

pub fn process_file_content(
    content: &str,
    sorted_aliases: &[(String, String)],
    skip_list: &[String],
    src_prefix: &str,
    root_dir: Option<&Path>,
    inverted_aliases: &[(String, String)],
) -> (String, Vec<RequireRewrite>) {
    let re = require_regex();
    let mut rewrites: Vec<RequireRewrite> = Vec::new();
    let mut rebuilt = String::with_capacity(content.len());
    let mut last_end = 0usize;

    for caps in re.captures_iter(content) {
        let Some(whole_match) = caps.get(0) else {
            continue;
        };
        let Some(path_match) = caps.get(1) else {
            continue;
        };

        rebuilt.push_str(&content[last_end..whole_match.start()]);

        let required_path = path_match.as_str();
        if let Some(new_path) = rewrite_require_path(
            required_path,
            sorted_aliases,
            skip_list,
            src_prefix,
            root_dir,
            inverted_aliases,
        ) {
            rebuilt.push_str("require(\"");
            rebuilt.push_str(&new_path);
            rebuilt.push_str("\")");
            rewrites.push(RequireRewrite {
                old: required_path.to_string(),
                new: new_path,
            });
        } else {
            rebuilt.push_str(whole_match.as_str());
        }

        last_end = whole_match.end();
    }

    if last_end == 0 {
        return (content.to_string(), rewrites);
    }

    rebuilt.push_str(&content[last_end..]);
    (rebuilt, rewrites)
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
        // Equal length → alphabetical by alias shortcut name (@Alpha/ < @Beta/)
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
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src", None, &[]);

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
        let (_, rewrites) = process_file_content(content, &sorted, &skip, "src", None, &[]);

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
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src", None, &[]);

        assert!(rewrites.is_empty(), "external alias must not be rewritten");
        assert_eq!(new_content, content, "content must be unchanged");
    }

    #[test]
    fn test_builtin_self_untouched() {
        let sorted: Vec<(String, String)> = vec![];
        let skip = build_skip_list(&HashMap::new(), "src");

        let content = r#"local m = require("@self/module")"#;
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src", None, &[]);

        assert!(rewrites.is_empty(), "@self require must not be rewritten");
        assert_eq!(new_content, content, "content must be unchanged");
    }

    #[test]
    fn test_builtin_game_untouched() {
        let sorted: Vec<(String, String)> = vec![];
        let skip = build_skip_list(&HashMap::new(), "src");

        let content = r#"local m = require("@game/ReplicatedStorage")"#;
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src", None, &[]);

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
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src", None, &[]);

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
        let (_, rewrites) = process_file_content(content, &sorted, &skip, "src", None, &[]);

        assert_eq!(rewrites.len(), 2, "both requires must be rewritten");
        let new_paths: Vec<&str> = rewrites.iter().map(|r| r.new.as_str()).collect();
        assert!(new_paths.contains(&"@Client/ui"), "Client rewrite must be present");
        assert!(new_paths.contains(&"@Server/api"), "Server rewrite must be present");
    }

    #[test]
    fn test_aliased_path_no_filesystem_untouched() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        
        let content = r#"local m = require("@Client/module")"#;
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src", None, &[]);

        assert!(rewrites.is_empty(), "aliased path with no filesystem must not be rewritten");
        assert_eq!(new_content, content, "content must be unchanged");
    }

    #[test]
    fn test_aliased_path_resolves_through_shortcut() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        let content = r#"local m = require("@Client/module")"#;
        let (new_content, rewrites) = process_file_content(content, &sorted, &skip, "src", None, &[]);

        // No filesystem to search — path stays the same
        assert!(rewrites.is_empty(), "aliased path with no filesystem should not be rewritten");
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

        // Small sleep to ensure mtime difference would be observable if file were written
        std::thread::sleep(std::time::Duration::from_millis(50));

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

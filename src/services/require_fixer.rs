use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use walkdir::WalkDir;

use crate::{output, services::rojo_project};

#[derive(Debug, Default)]
pub struct FixResult {
    pub files_changed: usize,
    pub total_files_scanned: usize,
    pub changes: Vec<FileChange>,
}

#[derive(Debug)]
pub struct FileChange {
    pub file: PathBuf,
    pub rewrites: Vec<RequireRewrite>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequireRewrite {
    pub old: String,
    pub new: String,
}

pub struct FixContext {
    pub sorted_aliases: Vec<(String, String)>,
    pub skip_list: Vec<String>,
    pub inverted_aliases: Vec<(String, String)>,
    pub game_alias_rewrites: Vec<(String, String)>,
    pub src_prefix: String,
}

impl FixContext {
    pub fn new(aliases: &HashMap<String, String>, src_prefix: &str) -> Self {
        Self::for_path(Path::new(src_prefix), aliases, src_prefix)
    }

    fn for_path(start_path: &Path, aliases: &HashMap<String, String>, src_prefix: &str) -> Self {
        let project_root = find_project_root(start_path);
        Self {
            sorted_aliases: build_sorted_src_aliases(aliases, src_prefix),
            skip_list: build_skip_list(aliases, src_prefix),
            inverted_aliases: build_inverted_aliases(aliases),
            game_alias_rewrites: build_game_alias_rewrites(
                project_root.as_deref(),
                aliases,
                src_prefix,
            ),
            src_prefix: src_prefix.to_string(),
        }
    }
}

pub fn fix_requires(
    root_dir: &Path,
    aliases: &HashMap<String, String>,
    src_prefix: &str,
) -> Result<FixResult> {
    let ctx = FixContext::for_path(root_dir, aliases, src_prefix);

    let files: Vec<PathBuf> = WalkDir::new(root_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_lua_file(e.path()))
        .map(|e| e.into_path())
        .collect();

    let total_files_scanned = files.len();

    let mut changes: Vec<FileChange> = Vec::new();
    for path in &files {
        let content = std::fs::read_to_string(path)?;
        if !content.contains("require(\"") {
            continue;
        }
        let (new_content, rewrites) = process_file_content(
            &content,
            &ctx.sorted_aliases,
            &ctx.skip_list,
            src_prefix,
            Some(root_dir),
            &ctx.inverted_aliases,
            &ctx.game_alias_rewrites,
        );
        if rewrites.is_empty() {
            continue;
        }
        std::fs::write(path, &new_content)?;
        changes.push(FileChange {
            file: path.clone(),
            rewrites,
        });
    }

    let files_changed = changes.len();
    Ok(FixResult {
        files_changed,
        total_files_scanned,
        changes,
    })
}

pub fn fix_single_file(
    file_path: &Path,
    aliases: &HashMap<String, String>,
    src_prefix: &str,
) -> Result<Option<FileChange>> {
    let ctx = FixContext::for_path(file_path, aliases, src_prefix);

    let content = std::fs::read_to_string(file_path)?;
    let (new_content, rewrites) = process_file_content(
        &content,
        &ctx.sorted_aliases,
        &ctx.skip_list,
        src_prefix,
        Some(Path::new(src_prefix)),
        &ctx.inverted_aliases,
        &ctx.game_alias_rewrites,
    );

    if rewrites.is_empty() {
        return Ok(None);
    }

    std::fs::write(file_path, &new_content)?;
    Ok(Some(FileChange {
        file: file_path.to_path_buf(),
        rewrites,
    }))
}

pub fn fix_single_file_with_context(
    file_path: &Path,
    ctx: &FixContext,
) -> Result<Option<FileChange>> {
    let content = std::fs::read_to_string(file_path)?;
    if !content.contains("require(\"") {
        return Ok(None);
    }
    let (new_content, rewrites) = process_file_content(
        &content,
        &ctx.sorted_aliases,
        &ctx.skip_list,
        &ctx.src_prefix,
        Some(Path::new(&ctx.src_prefix)),
        &ctx.inverted_aliases,
        &ctx.game_alias_rewrites,
    );

    if rewrites.is_empty() {
        return Ok(None);
    }

    std::fs::write(file_path, &new_content)?;
    Ok(Some(FileChange {
        file: file_path.to_path_buf(),
        rewrites,
    }))
}

fn normalize_separators(s: &str) -> String {
    s.replace('\\', "/")
}

fn find_project_root(start_path: &Path) -> Option<PathBuf> {
    let mut current = if start_path.is_file() {
        start_path.parent()
    } else {
        Some(start_path)
    };

    while let Some(dir) = current {
        let candidate = if dir.as_os_str().is_empty() {
            Path::new(".")
        } else {
            dir
        };

        if candidate.join("default.project.json").exists() || candidate.join("ezpm.toml").exists() {
            return Some(candidate.to_path_buf());
        }

        current = candidate.parent();
    }

    let cwd = Path::new(".");
    if cwd.join("default.project.json").exists() || cwd.join("ezpm.toml").exists() {
        return Some(cwd.to_path_buf());
    }

    None
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

    list.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    list
}

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

pub fn is_lua_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|s| s.to_str()),
        Some("lua") | Some("luau")
    )
}

pub fn require_regex() -> &'static Regex {
    static REQUIRE_RE: OnceLock<Regex> = OnceLock::new();
    REQUIRE_RE
        .get_or_init(|| Regex::new(r#"require\("([^"]+)"\)"#).expect("require regex is valid"))
}

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

    inverted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    inverted
}

fn build_game_alias_rewrites(
    project_root: Option<&Path>,
    aliases: &HashMap<String, String>,
    src_prefix: &str,
) -> Vec<(String, String)> {
    let mappings = match project_root {
        Some(project_root) => {
            rojo_project::alias_rojo_mappings_for_project_root(project_root, aliases, src_prefix)
        }
        None => rojo_project::default_alias_rojo_mappings(aliases, src_prefix),
    };

    let mut rewrites: Vec<(String, String)> = mappings
        .into_iter()
        .map(|mapping| {
            (
                format!("@game/{}", mapping.game_path()),
                format!("@{}", mapping.alias_name),
            )
        })
        .collect();

    rewrites.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.1.cmp(&b.1)));
    rewrites
}

fn convert_path_to_alias(file_path: &str, inverted_aliases: &[(String, String)]) -> String {
    let mut converted = normalize_separators(file_path);
    for (real_path, shortcut) in inverted_aliases {
        if converted.contains(real_path.as_str()) {
            converted = converted.replace(real_path.as_str(), shortcut.as_str());
            break;
        }
    }

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

fn rewrite_game_alias_path(
    required_path: &str,
    game_alias_rewrites: &[(String, String)],
) -> Option<String> {
    for (game_path, alias_path) in game_alias_rewrites {
        if required_path == game_path {
            return Some(alias_path.clone());
        }

        let Some(remainder) = required_path.strip_prefix(game_path) else {
            continue;
        };
        let Some(remainder) = remainder.strip_prefix('/') else {
            continue;
        };

        return Some(format!("{alias_path}/{remainder}"));
    }

    None
}

fn rewrite_require_path(
    required_path: &str,
    sorted_aliases: &[(String, String)],
    skip_list: &[String],
    src_prefix: &str,
    root_dir: Option<&Path>,
    inverted_aliases: &[(String, String)],
    game_alias_rewrites: &[(String, String)],
) -> Option<String> {
    if let Some(new_path) = rewrite_game_alias_path(required_path, game_alias_rewrites) {
        return Some(new_path);
    }

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
    game_alias_rewrites: &[(String, String)],
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
            game_alias_rewrites,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn standard_aliases() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("Client".to_string(), "src/client/".to_string());
        m.insert("Server".to_string(), "src/server/".to_string());
        m.insert("Shared".to_string(), "src/shared/".to_string());
        m.insert("Packages".to_string(), "Packages/".to_string());
        m.insert("ServerPackages".to_string(), "ServerPackages/".to_string());
        m
    }

    fn game_alias_rewrites_for(aliases: &HashMap<String, String>) -> Vec<(String, String)> {
        build_game_alias_rewrites(None, aliases, "src")
    }

    #[test]
    fn test_longest_match_wins() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("SharedClient".to_string(), "src/client/shared/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");

        assert!(
            sorted.len() >= 2,
            "expected 2 sorted aliases, got {:?}",
            sorted
        );
        assert!(
            sorted[0].1.len() >= sorted[1].1.len(),
            "longer path must sort first: {:?}",
            sorted
        );
        assert_eq!(
            sorted[0].0, "@SharedClient/",
            "SharedClient should be first due to longer path"
        );
    }

    #[test]
    fn test_sort_stability_on_equal_length() {
        let mut aliases = HashMap::new();
        aliases.insert("Alpha".to_string(), "src/alpha/".to_string());
        aliases.insert("Beta".to_string(), "src/beta_/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");

        assert_eq!(sorted.len(), 2, "both aliases should appear");
        assert_eq!(sorted[0].0, "@Alpha/", "Alpha should sort before Beta");
        assert_eq!(sorted[1].0, "@Beta/", "Beta should sort after Alpha");
    }

    #[test]
    fn test_only_src_aliases_in_sorted_list() {
        let aliases = standard_aliases();
        let sorted = build_sorted_src_aliases(&aliases, "src");

        let names: Vec<&str> = sorted.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            !names.contains(&"@Packages/"),
            "Packages must not appear in src aliases"
        );
        assert!(
            !names.contains(&"@ServerPackages/"),
            "ServerPackages must not appear in src aliases"
        );
        assert!(names.contains(&"@Client/"), "Client must appear");
        assert!(names.contains(&"@Server/"), "Server must appear");
        assert!(names.contains(&"@Shared/"), "Shared must appear");
    }

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

        assert!(
            skip.contains(&"@self/".to_string()),
            "@self/ must be in skip list"
        );
        assert!(
            skip.contains(&"@self".to_string()),
            "@self must be in skip list"
        );
        assert!(
            skip.contains(&"@game/".to_string()),
            "@game/ must be in skip list"
        );
        assert!(
            skip.contains(&"@game".to_string()),
            "@game must be in skip list"
        );
    }

    #[test]
    fn test_finds_double_quote_requires() {
        let content = r#"local m = require("@Client/module")"#;
        let re = require_regex();
        let caps: Vec<&str> = re
            .captures_iter(content)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        assert_eq!(
            caps,
            vec!["@Client/module"],
            "double-quote require must be found"
        );
    }

    #[test]
    fn test_ignores_single_quote_requires() {
        let content = "local m = require('@Client/module')";
        let re = require_regex();
        let caps: Vec<&str> = re
            .captures_iter(content)
            .filter_map(|c| c.get(1).map(|m| m.as_str()))
            .collect();
        assert!(
            caps.is_empty(),
            "single-quote require must NOT be matched (Luau parity)"
        );
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

    #[test]
    fn test_rewrites_src_path_to_alias() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        let content = r#"local m = require("src/client/module")"#;
        let (new_content, rewrites) =
            process_file_content(content, &sorted, &skip, "src", None, &[], &[]);

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
        let (_, rewrites) = process_file_content(content, &sorted, &skip, "src", None, &[], &[]);

        assert_eq!(rewrites.len(), 1);
        assert_eq!(
            rewrites[0].new, "@SharedClient/util",
            "SharedClient must win over Client"
        );
    }

    #[test]
    fn test_skipped_aliases_untouched() {
        let aliases = standard_aliases();
        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        let content = r#"local r = require("@Packages/rodux")"#;
        let (new_content, rewrites) =
            process_file_content(content, &sorted, &skip, "src", None, &[], &[]);

        assert!(rewrites.is_empty(), "external alias must not be rewritten");
        assert_eq!(new_content, content, "content must be unchanged");
    }

    #[test]
    fn test_builtin_self_untouched() {
        let sorted: Vec<(String, String)> = vec![];
        let skip = build_skip_list(&HashMap::new(), "src");

        let content = r#"local m = require("@self/module")"#;
        let (new_content, rewrites) =
            process_file_content(content, &sorted, &skip, "src", None, &[], &[]);

        assert!(rewrites.is_empty(), "@self require must not be rewritten");
        assert_eq!(new_content, content, "content must be unchanged");
    }

    #[test]
    fn test_rewrites_game_shared_path_to_alias() {
        let aliases = standard_aliases();
        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");
        let game_aliases = game_alias_rewrites_for(&aliases);

        let content = r#"local m = require("@game/ReplicatedStorage/Shared/Util")"#;
        let (new_content, rewrites) =
            process_file_content(content, &sorted, &skip, "src", None, &[], &game_aliases);

        assert_eq!(rewrites.len(), 1, "shared @game path should be rewritten");
        assert_eq!(rewrites[0].old, "@game/ReplicatedStorage/Shared/Util");
        assert_eq!(rewrites[0].new, "@Shared/Util");
        assert_eq!(new_content, r#"local m = require("@Shared/Util")"#);
    }

    #[test]
    fn test_rewrites_game_unknown_src_alias_to_alias() {
        let mut aliases = standard_aliases();
        aliases.insert("Libraries".to_string(), "src/libraries/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");
        let game_aliases = game_alias_rewrites_for(&aliases);

        let content = r#"local m = require("@game/ReplicatedStorage/Libraries/Trove")"#;
        let (new_content, rewrites) =
            process_file_content(content, &sorted, &skip, "src", None, &[], &game_aliases);

        assert_eq!(rewrites.len(), 1, "custom src alias should be rewritten");
        assert_eq!(rewrites[0].new, "@Libraries/Trove");
        assert_eq!(new_content, r#"local m = require("@Libraries/Trove")"#);
    }

    #[test]
    fn test_rewrites_game_client_path_to_alias() {
        let aliases = standard_aliases();
        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");
        let game_aliases = game_alias_rewrites_for(&aliases);

        let content = r#"local m = require("@game/StarterPlayer/StarterPlayerScripts/Client/Foo")"#;
        let (new_content, rewrites) =
            process_file_content(content, &sorted, &skip, "src", None, &[], &game_aliases);

        assert_eq!(rewrites.len(), 1, "client @game path should be rewritten");
        assert_eq!(rewrites[0].new, "@Client/Foo");
        assert_eq!(new_content, r#"local m = require("@Client/Foo")"#);
    }

    #[test]
    fn test_unmatched_game_path_is_untouched() {
        let aliases = standard_aliases();
        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");
        let game_aliases = game_alias_rewrites_for(&aliases);

        let content = r#"local m = require("@game/Workspace/Foo")"#;
        let (new_content, rewrites) =
            process_file_content(content, &sorted, &skip, "src", None, &[], &game_aliases);

        assert!(
            rewrites.is_empty(),
            "unmatched @game path must remain unchanged"
        );
        assert_eq!(new_content, content, "content must be unchanged");
    }

    #[test]
    fn test_bare_game_is_untouched() {
        let aliases = standard_aliases();
        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");
        let game_aliases = game_alias_rewrites_for(&aliases);

        let content = r#"local m = require("@game")"#;
        let (new_content, rewrites) =
            process_file_content(content, &sorted, &skip, "src", None, &[], &game_aliases);

        assert!(rewrites.is_empty(), "bare @game must remain unchanged");
        assert_eq!(new_content, content, "content must be unchanged");
    }

    #[test]
    fn test_no_change_returns_empty_rewrites() {
        let aliases = standard_aliases();
        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        let content = "local x = 42\nreturn x\n";
        let (new_content, rewrites) =
            process_file_content(content, &sorted, &skip, "src", None, &[], &[]);

        assert!(
            rewrites.is_empty(),
            "no rewrites for content with no requires"
        );
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
        let (_, rewrites) = process_file_content(content, &sorted, &skip, "src", None, &[], &[]);

        assert_eq!(rewrites.len(), 2, "both requires must be rewritten");
        let new_paths: Vec<&str> = rewrites.iter().map(|r| r.new.as_str()).collect();
        assert!(
            new_paths.contains(&"@Client/ui"),
            "Client rewrite must be present"
        );
        assert!(
            new_paths.contains(&"@Server/api"),
            "Server rewrite must be present"
        );
    }

    #[test]
    fn test_aliased_path_no_filesystem_untouched() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        let content = r#"local m = require("@Client/module")"#;
        let (new_content, rewrites) =
            process_file_content(content, &sorted, &skip, "src", None, &[], &[]);

        assert!(
            rewrites.is_empty(),
            "aliased path with no filesystem must not be rewritten"
        );
        assert_eq!(new_content, content, "content must be unchanged");
    }

    #[test]
    fn test_aliased_path_resolves_through_shortcut() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());

        let sorted = build_sorted_src_aliases(&aliases, "src");
        let skip = build_skip_list(&aliases, "src");

        let content = r#"local m = require("@Client/module")"#;
        let (new_content, rewrites) =
            process_file_content(content, &sorted, &skip, "src", None, &[], &[]);

        assert!(
            rewrites.is_empty(),
            "aliased path with no filesystem should not be rewritten"
        );
        assert_eq!(new_content, content, "content must be unchanged");
    }

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

        assert_eq!(
            result.total_files_scanned, 3,
            "all 3 .luau files must be scanned"
        );
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

        assert_eq!(
            result.total_files_scanned, 2,
            ".txt and .json must not be scanned"
        );
    }

    #[test]
    fn test_unchanged_files_not_written() {
        let dir = make_temp_tree(&[("no_requires.luau", "local x = 42\nreturn x\n")]);

        let file_path = dir.path().join("no_requires.luau");
        let mtime_before = std::fs::metadata(&file_path)
            .expect("metadata")
            .modified()
            .expect("mtime");

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
            ("b.luau", "local y = 42\n"),
            ("c.luau", r#"local z = require("src/server/z")"#),
        ]);

        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("Server".to_string(), "src/server/".to_string());

        let result = fix_requires(dir.path(), &aliases, "src").expect("fix_requires must succeed");

        assert_eq!(result.total_files_scanned, 3, "3 files scanned");
        assert_eq!(
            result.files_changed, 2,
            "only 2 files changed (b.luau has no requires)"
        );
        assert_eq!(result.changes.len(), 2, "2 FileChange entries");
    }

    #[test]
    fn test_fix_requires_rewrites_unchanged_dependents_after_topology_change() {
        let dir = make_temp_tree(&[
            ("consumer.luau", r#"local m = require("@Client/module")"#),
            ("src/shared/module.luau", "return {}\n"),
        ]);

        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("Shared".to_string(), "src/shared/".to_string());
        aliases.insert("Server".to_string(), "src/server/".to_string());

        fix_requires(dir.path(), &aliases, "src").expect("initial scan must succeed");

        let consumer_path = dir.path().join("consumer.luau");
        let initial = std::fs::read_to_string(&consumer_path).expect("read initial consumer");
        assert!(
            initial.contains("@Shared/module"),
            "initial scan should resolve the consumer to the shared alias: {initial}"
        );

        std::fs::create_dir_all(dir.path().join("src/server")).expect("create server dir");
        std::fs::rename(
            dir.path().join("src/shared/module.luau"),
            dir.path().join("src/server/module.luau"),
        )
        .expect("move module to server");

        fix_requires(dir.path(), &aliases, "src").expect("rescan after move must succeed");

        let updated = std::fs::read_to_string(&consumer_path).expect("read updated consumer");
        assert!(
            updated.contains("@Server/module"),
            "rescan must rewrite unchanged dependents after topology changes: {updated}"
        );
    }
}

use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
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

#[derive(Clone)]
pub struct FixContext {
    pub sorted_aliases: Vec<(String, String)>,
    pub skip_list: Vec<String>,
    pub inverted_aliases: Vec<(String, String)>,
    pub game_alias_rewrites: Vec<(String, String)>,
    pub src_prefix: String,
    source_mappings: Vec<rojo_project::AliasRojoMapping>,
}

#[derive(Debug, Default)]
pub struct ModuleIndex {
    by_name: HashMap<String, Vec<PathBuf>>,
    warned_ambiguities: Mutex<std::collections::HashSet<String>>,
}

impl ModuleIndex {
    pub fn build(root_dir: &Path) -> Self {
        Self::from_files(&lua_files(root_dir))
    }

    pub fn from_files(files: &[PathBuf]) -> Self {
        let mut index = Self::default();
        for path in files {
            index.insert(path);
        }
        index
    }

    fn insert(&mut self, path: &Path) {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            return;
        };

        self.add(file_name, path);
        self.add(stem, path);
        if stem == "init" {
            if let Some(parent) = path
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
            {
                if parent != stem {
                    self.add(parent, path);
                }
            }
        }
    }

    fn add(&mut self, name: &str, path: &Path) {
        self.by_name
            .entry(name.to_string())
            .or_default()
            .push(path.to_path_buf());
    }

    fn find_unique(&self, name: &str) -> Option<&Path> {
        let matches = self.by_name.get(name)?;
        if matches.len() == 1 {
            return matches.first().map(PathBuf::as_path);
        }

        let should_warn = self
            .warned_ambiguities
            .lock()
            .is_ok_and(|mut warned| warned.insert(name.to_string()));
        if should_warn {
            output::warn(&format!(
                "require path '{name}' is ambiguous; matches {}",
                matches
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        None
    }
}

impl FixContext {
    pub fn new_for_project(
        project_root: &Path,
        project_file: &Path,
        aliases: &HashMap<String, String>,
        src_prefix: &str,
    ) -> Self {
        let source_mappings = rojo_project::alias_rojo_mappings_for_project(
            project_root,
            project_file,
            aliases,
            src_prefix,
        );
        Self::from_mappings(aliases, src_prefix, source_mappings)
    }

    pub fn from_mappings(
        aliases: &HashMap<String, String>,
        src_prefix: &str,
        source_mappings: Vec<rojo_project::AliasRojoMapping>,
    ) -> Self {
        Self {
            sorted_aliases: build_sorted_src_aliases(aliases, src_prefix),
            skip_list: build_skip_list(aliases, src_prefix),
            inverted_aliases: build_inverted_aliases(aliases),
            game_alias_rewrites: game_alias_rewrites_from_mappings(&source_mappings),
            src_prefix: src_prefix.to_string(),
            source_mappings,
        }
    }

    pub fn game_require_for_source(
        &self,
        project_dir: &Path,
        source_path: &Path,
    ) -> Option<String> {
        game_require_from_mappings(project_dir, source_path, &self.source_mappings)
    }
}

pub fn fix_requires_with_context(root_dir: &Path, ctx: &FixContext) -> Result<FixResult> {
    let files = lua_files(root_dir);
    let total_files_scanned = files.len();
    let module_index = ModuleIndex::from_files(&files);

    let mut changes: Vec<FileChange> = Vec::new();
    for path in &files {
        let content = std::fs::read_to_string(path)?;
        if !content.contains("require(\"") {
            continue;
        }
        let (new_content, rewrites) =
            process_file_content_with_index(&content, ctx, Some(&module_index));
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

pub fn fix_single_file_with_index(
    file_path: &Path,
    ctx: &FixContext,
    module_index: &ModuleIndex,
) -> Result<Option<FileChange>> {
    let content = std::fs::read_to_string(file_path)?;
    if !content.contains("require(\"") {
        return Ok(None);
    }
    let (new_content, rewrites) =
        process_file_content_with_index(&content, ctx, Some(module_index));

    if rewrites.is_empty() {
        return Ok(None);
    }

    std::fs::write(file_path, &new_content)?;
    Ok(Some(FileChange {
        file: file_path.to_path_buf(),
        rewrites,
    }))
}

fn game_require_from_mappings(
    project_dir: &Path,
    source_path: &Path,
    mappings: &[rojo_project::AliasRojoMapping],
) -> Option<String> {
    let source = source_path.strip_prefix(project_dir).unwrap_or(source_path);
    let source = normalize_separators(&source.to_string_lossy());
    let mut best_length = 0;
    let mut best_path = None;

    for mapping in mappings {
        let alias_path = mapping.alias_path.trim_end_matches('/');
        let relative = if source == alias_path {
            ""
        } else if let Some(relative) = source.strip_prefix(&format!("{alias_path}/")) {
            relative
        } else {
            continue;
        };
        if alias_path.len() <= best_length {
            continue;
        }
        let mut path = mapping.instance_path.clone();
        path.extend(
            strip_module_suffix(relative)
                .split('/')
                .filter(|component| !component.is_empty())
                .map(str::to_string),
        );
        best_length = alias_path.len();
        best_path = Some(rojo_project::game_require(&path));
    }
    best_path
}

pub fn rewrite_require_prefixes(
    root_dir: &Path,
    replacements: &[(String, String)],
) -> Result<usize> {
    if replacements.is_empty() {
        return Ok(0);
    }

    let replacements = replacements
        .iter()
        .map(|(old, new)| (old.as_str(), new.as_str()))
        .collect::<HashMap<_, _>>();
    let mut changed = 0;
    for path in lua_files(root_dir) {
        let content = std::fs::read_to_string(&path)?;
        if !content.contains("require(\"") {
            continue;
        }
        let mut rewrites = Vec::new();
        for captures in require_regex().captures_iter(&content) {
            let whole = captures.get(0).expect("require match");
            let path = captures.get(1).expect("require path").as_str();
            if let Some((old_path, new_path)) = matching_prefix(path, &replacements) {
                let suffix = &path[old_path.len()..];
                rewrites.push((
                    whole.start(),
                    whole.end(),
                    format!("require(\"{new_path}{suffix}\")"),
                ));
            }
        }
        if !rewrites.is_empty() {
            let mut updated = content;
            for (start, end, replacement) in rewrites.into_iter().rev() {
                updated.replace_range(start..end, &replacement);
            }
            std::fs::write(&path, updated)?;
            changed += 1;
        }
    }
    Ok(changed)
}

fn matching_prefix<'a>(
    path: &'a str,
    replacements: &'a HashMap<&str, &str>,
) -> Option<(&'a str, &'a str)> {
    let mut candidate = path;
    loop {
        if let Some(new_path) = replacements.get(candidate) {
            return Some((candidate, *new_path));
        }
        let slash = candidate.rfind('/')?;
        candidate = &candidate[..slash];
    }
}

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

pub fn lua_files(root_dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(root_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && is_lua_file(entry.path()))
        .map(|entry| entry.into_path())
        .collect()
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

fn game_alias_rewrites_from_mappings(
    mappings: &[rojo_project::AliasRojoMapping],
) -> Vec<(String, String)> {
    let mut rewrites: Vec<(String, String)> = mappings
        .iter()
        .map(|mapping| {
            let relative = mapping
                .alias_path
                .strip_prefix(&mapping.alias_root)
                .unwrap_or_default()
                .trim_start_matches('/');
            let alias_path = if relative.is_empty() {
                format!("@{}", mapping.alias_name)
            } else {
                format!("@{}/{relative}", mapping.alias_name)
            };
            (
                alias_path,
                rojo_project::game_require(&mapping.instance_path),
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
            if let Some(alias_start) = converted.find('@') {
                converted = converted[alias_start..].to_string();
            }
            break;
        }
    }

    converted.truncate(strip_module_suffix(&converted).len());
    converted
}

fn strip_module_suffix(path: &str) -> &str {
    for init in ["init.luau", "init.lua", "init"] {
        if let Some(stem) = path.strip_suffix(init) {
            if stem.is_empty() {
                return stem;
            }
            if let Some(parent) = stem.strip_suffix('/') {
                return parent;
            }
        }
    }

    path.strip_suffix(".luau")
        .or_else(|| path.strip_suffix(".lua"))
        .unwrap_or(path)
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

fn rewrite_alias_as_game_path(
    required_path: &str,
    game_alias_rewrites: &[(String, String)],
) -> Option<String> {
    for (alias_path, game_path) in game_alias_rewrites {
        if required_path == alias_path {
            return Some(game_path.clone());
        }

        let Some(remainder) = required_path.strip_prefix(alias_path) else {
            continue;
        };
        let Some(remainder) = remainder.strip_prefix('/') else {
            continue;
        };

        return Some(format!("{game_path}/{remainder}"));
    }

    None
}

fn rewrite_require_path(
    required_path: &str,
    ctx: &FixContext,
    module_index: Option<&ModuleIndex>,
) -> Option<String> {
    if required_path.starts_with("@game/") {
        if let Some(index) = module_index {
            if let Some(file_name) = required_path.rsplit('/').next() {
                if let Some(found) = index.find_unique(file_name) {
                    let found_str = normalize_separators(&found.to_string_lossy());
                    let alias_path = convert_path_to_alias(&found_str, &ctx.inverted_aliases);
                    if let Some(game_path) =
                        rewrite_alias_as_game_path(&alias_path, &ctx.game_alias_rewrites)
                    {
                        if game_path != required_path {
                            return Some(game_path);
                        }
                    }
                }
            }
        }
        return None;
    }

    if ctx
        .skip_list
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

    for (alias_shortcut, real_path) in &ctx.sorted_aliases {
        if required_path.starts_with(real_path.as_str()) {
            let alias_path = format!("{}{}", alias_shortcut, &required_path[real_path.len()..]);
            return rewrite_alias_as_game_path(&alias_path, &ctx.game_alias_rewrites)
                .or(Some(alias_path));
        }
    }

    let src_slash = format!("{}/", ctx.src_prefix);
    if required_path.starts_with(&src_slash) || required_path == ctx.src_prefix {
        if let Some(index) = module_index {
            if let Some(file_name) = required_path.rsplit('/').next() {
                if let Some(found) = index.find_unique(file_name) {
                    let found_str = normalize_separators(&found.to_string_lossy());
                    let converted = convert_path_to_alias(&found_str, &ctx.inverted_aliases);
                    let converted =
                        rewrite_alias_as_game_path(&converted, &ctx.game_alias_rewrites)
                            .unwrap_or(converted);
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
    for (shortcut, real_path) in &ctx.sorted_aliases {
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

    if let Some(index) = module_index {
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

        if Path::new(&full_path).is_file() || Path::new(&init_path).is_file() {
            if let Some(game_path) =
                rewrite_alias_as_game_path(required_path, &ctx.game_alias_rewrites)
            {
                return Some(game_path);
            }
        } else if let Some(file_name) = as_file_path.rsplit('/').next() {
            if let Some(found) = index.find_unique(file_name) {
                let found_str = normalize_separators(&found.to_string_lossy());
                let converted = convert_path_to_alias(&found_str, &ctx.inverted_aliases);
                let converted = rewrite_alias_as_game_path(&converted, &ctx.game_alias_rewrites)
                    .unwrap_or(converted);
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

    None
}

pub fn process_file_content(
    content: &str,
    ctx: &FixContext,
    root_dir: Option<&Path>,
) -> (String, Vec<RequireRewrite>) {
    let module_index = root_dir.map(ModuleIndex::build);
    process_file_content_with_index(content, ctx, module_index.as_ref())
}

fn process_file_content_with_index(
    content: &str,
    ctx: &FixContext,
    module_index: Option<&ModuleIndex>,
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
        if let Some(new_path) = rewrite_require_path(required_path, ctx, module_index) {
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

    fn ctx_without_game_paths(aliases: &HashMap<String, String>) -> FixContext {
        FixContext::from_mappings(aliases, "src", Vec::new())
    }

    fn ctx_with_game_paths(aliases: &HashMap<String, String>) -> FixContext {
        FixContext::from_mappings(
            aliases,
            "src",
            rojo_project::default_alias_rojo_mappings(aliases, "src"),
        )
    }

    #[test]
    fn test_longest_match_applied() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("SharedClient".to_string(), "src/client/shared/".to_string());

        let content = r#"local m = require("src/client/shared/util")"#;
        let (_, rewrites) = process_file_content(content, &ctx_without_game_paths(&aliases), None);

        assert_eq!(rewrites.len(), 1);
        assert_eq!(
            rewrites[0].new, "@SharedClient/util",
            "SharedClient must win over Client"
        );
    }

    #[test]
    fn test_rewrites_source_path_to_game_path() {
        let aliases = standard_aliases();

        let content = r#"local m = require("src/shared/Util")"#;
        let (new_content, rewrites) =
            process_file_content(content, &ctx_with_game_paths(&aliases), None);

        assert_eq!(rewrites.len(), 1);
        assert_eq!(rewrites[0].new, "@game/ReplicatedStorage/Shared/Util");
        assert_eq!(
            new_content,
            r#"local m = require("@game/ReplicatedStorage/Shared/Util")"#
        );
    }

    #[test]
    fn test_rewrites_project_mapped_alias_subdirectory() {
        let aliases = HashMap::from([("Shared".to_string(), "src/shared/".to_string())]);
        let mappings = vec![rojo_project::AliasRojoMapping {
            alias_name: "Shared".to_string(),
            alias_root: "src/shared".to_string(),
            alias_path: "src/shared/features".to_string(),
            instance_path: vec!["ReplicatedStorage".to_string(), "Features".to_string()],
        }];
        let context = FixContext::from_mappings(&aliases, "src", mappings);
        let dir = make_temp_tree(&[("src/shared/features/Signal.luau", "return {}\n")]);

        let content = r#"local m = require("@Shared/features/Signal")"#;
        let (updated, rewrites) =
            process_file_content(content, &context, Some(dir.path().join("src").as_path()));

        assert_eq!(rewrites.len(), 1);
        assert_eq!(
            updated,
            r#"local m = require("@game/ReplicatedStorage/Features/Signal")"#
        );
    }

    #[test]
    fn test_multiple_rewrites_in_one_file() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("Server".to_string(), "src/server/".to_string());

        let content = r#"
local a = require("src/client/ui")
local b = require("src/server/api")
"#;
        let (_, rewrites) = process_file_content(content, &ctx_without_game_paths(&aliases), None);

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
    fn test_unchanged_files_not_written() {
        let dir = make_temp_tree(&[("no_requires.luau", "local x = 42\nreturn x\n")]);

        let file_path = dir.path().join("no_requires.luau");
        let mtime_before = std::fs::metadata(&file_path)
            .expect("metadata")
            .modified()
            .expect("mtime");

        std::thread::sleep(std::time::Duration::from_millis(50));

        let aliases = standard_aliases();
        fix_requires_with_context(dir.path(), &ctx_with_game_paths(&aliases))
            .expect("fix_requires must succeed");

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

        fix_requires_with_context(dir.path(), &ctx_with_game_paths(&aliases))
            .expect("fix_requires must succeed");

        let updated = std::fs::read_to_string(&file_path).expect("read updated file");
        assert!(
            updated
                .contains(r#"require("@game/StarterPlayer/StarterPlayerScripts/Client/module")"#),
            "file content must be updated on disk: {updated}"
        );
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

        let context = ctx_with_game_paths(&aliases);
        fix_requires_with_context(dir.path(), &context).expect("initial scan must succeed");

        let consumer_path = dir.path().join("consumer.luau");
        let initial = std::fs::read_to_string(&consumer_path).expect("read initial consumer");
        assert!(
            initial.contains("@game/ReplicatedStorage/Shared/module"),
            "initial scan should resolve the consumer to the shared module: {initial}"
        );

        std::fs::create_dir_all(dir.path().join("src/server")).expect("create server dir");
        std::fs::rename(
            dir.path().join("src/shared/module.luau"),
            dir.path().join("src/server/module.luau"),
        )
        .expect("move module to server");

        fix_requires_with_context(dir.path(), &context).expect("rescan after move must succeed");

        let updated = std::fs::read_to_string(&consumer_path).expect("read updated consumer");
        assert!(
            updated.contains("@game/ServerScriptService/Server/module"),
            "rescan must rewrite unchanged dependents after topology changes: {updated}"
        );
    }
}

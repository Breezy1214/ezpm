use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use walkdir::WalkDir;

use crate::services::sourcemap::SourcemapIndex;

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
    alias_paths: HashMap<String, String>,
    project_root: PathBuf,
    sourcemap: SourcemapIndex,
}

#[derive(Debug, Default)]
pub struct ModuleIndex {
    files: Vec<PathBuf>,
    by_name: HashMap<String, Vec<usize>>,
    ambiguities: Mutex<HashMap<String, Vec<PathBuf>>>,
}

impl ModuleIndex {
    pub fn build(root_dir: &Path) -> Self {
        Self::from_files(&lua_files(root_dir))
    }

    pub fn from_files(files: &[PathBuf]) -> Self {
        let mut index = Self::default();
        for path in files {
            index.insert(path.clone());
        }
        index
    }

    fn insert(&mut self, path: PathBuf) {
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            return;
        };
        let file_name = file_name.to_string();
        let stem = stem.to_string();
        let parent_name = if stem == "init" {
            path.parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .filter(|name| *name != "init")
                .map(str::to_string)
        } else {
            None
        };
        let index = self.files.len();
        self.files.push(path);

        self.add(file_name, index);
        self.add(stem, index);
        if let Some(parent) = parent_name {
            self.add(parent, index);
        }
    }

    fn add(&mut self, name: String, index: usize) {
        self.by_name.entry(name).or_default().push(index);
    }

    fn find_unique(&self, name: &str) -> Option<&Path> {
        let matches = self.by_name.get(name)?;
        if matches.len() == 1 {
            return matches
                .first()
                .and_then(|index| self.files.get(*index))
                .map(PathBuf::as_path);
        }

        if let Ok(mut ambiguities) = self.ambiguities.lock() {
            ambiguities.insert(
                name.to_string(),
                matches
                    .iter()
                    .filter_map(|index| self.files.get(*index).cloned())
                    .collect(),
            );
        }
        None
    }

    fn ambiguity_error(&self) -> Option<anyhow::Error> {
        let mut stored = self.ambiguities.lock().ok()?;
        let ambiguities = std::mem::take(&mut *stored);
        if ambiguities.is_empty() {
            return None;
        }

        let mut entries = ambiguities.into_iter().collect::<Vec<_>>();
        entries.sort_by(|(left, _), (right, _)| left.cmp(right));
        let details = entries
            .into_iter()
            .map(|(name, paths)| {
                let mut paths = paths
                    .into_iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>();
                paths.sort();
                format!("require(\"{name}\") matches {}", paths.join(", "))
            })
            .collect::<Vec<_>>()
            .join("; ");
        Some(anyhow::anyhow!(
            "ambiguous module name; use an alias or source path: {details}"
        ))
    }
}

impl FixContext {
    pub fn new(
        project_root: &Path,
        aliases: &HashMap<String, String>,
        sourcemap: SourcemapIndex,
    ) -> Self {
        let alias_paths = aliases
            .iter()
            .map(|(name, path)| (format!("@{name}"), normalize_separators(path)))
            .collect();
        Self {
            alias_paths,
            project_root: project_root.to_path_buf(),
            sourcemap,
        }
    }

    pub fn with_aliases(&self, aliases: &HashMap<String, String>) -> Self {
        Self::new(&self.project_root, aliases, self.sourcemap.clone())
    }

    pub fn game_require_for_source(&self, source_path: &Path) -> Option<String> {
        self.sourcemap.game_path(source_path).map(str::to_string)
    }

    pub fn relocation_rewrites(
        &self,
        current: &Self,
        from: &Path,
        to: &Path,
    ) -> Vec<(String, String)> {
        if let (Some(old), Some(new)) = (
            self.game_require_for_source(from),
            current.game_require_for_source(to),
        ) {
            return (old != new).then_some((old, new)).into_iter().collect();
        }

        self.sourcemap
            .source_files()
            .iter()
            .filter_map(|old_source| {
                let relative = old_source.strip_prefix(from).ok()?;
                let new_source = to.join(relative);
                let old = self.sourcemap.game_path(old_source)?;
                let new = current.sourcemap.game_path(&new_source)?;
                (old != new).then(|| (old.to_string(), new.to_string()))
            })
            .collect()
    }

    pub fn module_index(&self) -> ModuleIndex {
        ModuleIndex::from_files(self.sourcemap.source_files())
    }

    pub fn script_files(&self) -> &[PathBuf] {
        self.sourcemap.script_files()
    }
}

pub fn fix_requires_with_context(ctx: &FixContext) -> Result<FixResult> {
    let files = ctx.script_files();
    let total_files_scanned = files.len();
    let module_index = ctx.module_index();

    let mut pending = Vec::new();
    for path in files {
        let content = std::fs::read_to_string(path)?;
        if !content.contains("require(\"") {
            continue;
        }
        let (new_content, rewrites) =
            process_file_content_with_index(&content, ctx, Some(&module_index));
        let Some(new_content) = new_content else {
            continue;
        };
        pending.push((path, new_content, rewrites));
    }
    if let Some(error) = module_index.ambiguity_error() {
        return Err(error);
    }

    let mut changes = Vec::with_capacity(pending.len());
    for (path, content, rewrites) in pending {
        std::fs::write(path, content)?;
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

    if let Some(error) = module_index.ambiguity_error() {
        return Err(error);
    }

    let Some(new_content) = new_content else {
        return Ok(None);
    };

    std::fs::write(file_path, new_content)?;
    Ok(Some(FileChange {
        file: file_path.to_path_buf(),
        rewrites,
    }))
}

pub fn rewrite_require_prefixes(
    files: &[PathBuf],
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
    for path in files {
        let content = std::fs::read_to_string(path)?;
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
            std::fs::write(path, updated)?;
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

fn rewrite_require_path(
    required_path: &str,
    ctx: &FixContext,
    module_index: Option<&ModuleIndex>,
) -> Option<String> {
    if required_path.starts_with("@game/") {
        if ctx.sourcemap.contains_game_path(required_path) {
            return None;
        }
    } else if required_path == "@game"
        || required_path == "@self"
        || required_path.starts_with("@self/")
        || required_path.starts_with("./")
        || required_path.starts_with("../")
    {
        return None;
    } else if !required_path.contains('/') && !required_path.starts_with('@') {
        return game_path_for_module_name(required_path, required_path, ctx, module_index);
    } else if let Some(game_path) = ctx
        .sourcemap
        .game_path_for_logical_source(&ctx.project_root, required_path)
    {
        return Some(game_path.to_string());
    } else if let Some(logical_path) = resolve_alias(required_path, &ctx.alias_paths) {
        if let Some(game_path) = ctx
            .sourcemap
            .game_path_for_logical_source(&ctx.project_root, &logical_path)
        {
            return Some(game_path.to_string());
        }
    }

    let file_name = required_path.rsplit('/').next()?;
    game_path_for_module_name(file_name, required_path, ctx, module_index)
}

fn game_path_for_module_name(
    file_name: &str,
    required_path: &str,
    ctx: &FixContext,
    module_index: Option<&ModuleIndex>,
) -> Option<String> {
    let source_path = module_index?.find_unique(file_name)?;
    let game_path = ctx.sourcemap.game_path(source_path)?;
    (game_path != required_path).then(|| game_path.to_string())
}

fn resolve_alias(required_path: &str, aliases: &HashMap<String, String>) -> Option<String> {
    let (alias, relative) = required_path
        .split_once('/')
        .map_or((required_path, None), |(alias, relative)| {
            (alias, Some(relative))
        });
    let source_path = aliases.get(alias)?.trim_end_matches('/');
    match relative {
        Some(relative) => Some(format!("{source_path}/{relative}")),
        None => Some(source_path.to_string()),
    }
}

pub fn process_file_content(
    content: &str,
    ctx: &FixContext,
    root_dir: Option<&Path>,
) -> (String, Vec<RequireRewrite>) {
    let module_index = root_dir.map(ModuleIndex::build);
    let (updated, rewrites) = process_file_content_with_index(content, ctx, module_index.as_ref());
    (updated.unwrap_or_else(|| content.to_string()), rewrites)
}

fn process_file_content_with_index(
    content: &str,
    ctx: &FixContext,
    module_index: Option<&ModuleIndex>,
) -> (Option<String>, Vec<RequireRewrite>) {
    let re = require_regex();
    let mut rewrites: Vec<RequireRewrite> = Vec::new();
    let mut rebuilt: Option<String> = None;
    let mut last_end = 0usize;

    for caps in re.captures_iter(content) {
        let Some(whole_match) = caps.get(0) else {
            continue;
        };
        let Some(path_match) = caps.get(1) else {
            continue;
        };

        let required_path = path_match.as_str();
        if let Some(new_path) = rewrite_require_path(required_path, ctx, module_index) {
            let output = rebuilt.get_or_insert_with(|| String::with_capacity(content.len()));
            output.push_str(&content[last_end..whole_match.start()]);
            output.push_str("require(\"");
            output.push_str(&new_path);
            output.push_str("\")");
            rewrites.push(RequireRewrite {
                old: required_path.to_string(),
                new: new_path,
            });
            last_end = whole_match.end();
        } else if let Some(output) = &mut rebuilt {
            output.push_str(&content[last_end..whole_match.end()]);
            last_end = whole_match.end();
        }
    }

    if let Some(output) = &mut rebuilt {
        output.push_str(&content[last_end..]);
    }
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

    fn context(
        project_root: &Path,
        aliases: &HashMap<String, String>,
        pairs: &[(PathBuf, &str)],
    ) -> FixContext {
        let alias_paths = aliases
            .iter()
            .map(|(name, path)| (format!("@{name}"), normalize_separators(path)))
            .collect();
        let mut sourcemap = SourcemapIndex::from_pairs(pairs);
        sourcemap.add_script_files(&lua_files(project_root));
        FixContext {
            alias_paths,
            project_root: project_root.to_path_buf(),
            sourcemap,
        }
    }

    #[test]
    fn test_rewrites_source_path_to_game_path() {
        let aliases = standard_aliases();
        let dir = make_temp_tree(&[("src/shared/Util.luau", "return {}\n")]);
        let source = dir.path().join("src/shared/Util.luau");
        let context = context(
            dir.path(),
            &aliases,
            &[(source, "@game/ReplicatedStorage/Shared/Util")],
        );

        let content = r#"local m = require("src/shared/Util")"#;
        let (new_content, rewrites) = process_file_content(content, &context, None);

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
        let dir = make_temp_tree(&[("src/shared/features/Signal.luau", "return {}\n")]);
        let source = dir.path().join("src/shared/features/Signal.luau");
        let context = context(
            dir.path(),
            &aliases,
            &[(source, "@game/ReplicatedStorage/Features/Signal")],
        );

        let content = r#"local m = require("@Shared/features/Signal")"#;
        let (updated, rewrites) =
            process_file_content(content, &context, Some(dir.path().join("src").as_path()));

        assert_eq!(rewrites.len(), 1);
        assert_eq!(
            updated,
            r#"local m = require("@game/ReplicatedStorage/Features/Signal")"#
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
        let context = context(dir.path(), &aliases, &[]);
        fix_requires_with_context(&context).expect("fix_requires must succeed");

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
        let dir = make_temp_tree(&[
            (
                "has_require.luau",
                r#"local m = require("src/client/module")"#,
            ),
            ("src/client/module.luau", "return {}\n"),
        ]);

        let file_path = dir.path().join("has_require.luau");

        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        let module = dir.path().join("src/client/module.luau");
        let context = context(
            dir.path(),
            &aliases,
            &[(
                module,
                "@game/StarterPlayer/StarterPlayerScripts/Client/module",
            )],
        );

        fix_requires_with_context(&context).expect("fix_requires must succeed");

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

        let shared_module = dir.path().join("src/shared/module.luau");
        let initial_context = context(
            dir.path(),
            &aliases,
            &[(shared_module, "@game/ReplicatedStorage/Shared/module")],
        );
        fix_requires_with_context(&initial_context).expect("initial scan must succeed");

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

        let server_module = dir.path().join("src/server/module.luau");
        let moved_context = context(
            dir.path(),
            &aliases,
            &[(server_module, "@game/ServerScriptService/Server/module")],
        );
        fix_requires_with_context(&moved_context).expect("rescan after move must succeed");

        let updated = std::fs::read_to_string(&consumer_path).expect("read updated consumer");
        assert!(
            updated.contains("@game/ServerScriptService/Server/module"),
            "rescan must rewrite unchanged dependents after topology changes: {updated}"
        );
    }

    #[test]
    fn directory_relocation_uses_old_and_new_sourcemaps() {
        let dir = make_temp_tree(&[("src/old/Util.luau", "return {}\n")]);
        let aliases = HashMap::new();
        let old_source = dir.path().join("src/old/Util.luau");
        let old = context(
            dir.path(),
            &aliases,
            &[(old_source, "@game/ReplicatedStorage/Old/Util")],
        );

        std::fs::rename(dir.path().join("src/old"), dir.path().join("src/new"))
            .expect("rename directory");
        let new_source = dir.path().join("src/new/Util.luau");
        let current = context(
            dir.path(),
            &aliases,
            &[(new_source, "@game/ReplicatedStorage/New/Util")],
        );

        assert_eq!(
            old.relocation_rewrites(
                &current,
                &dir.path().join("src/old"),
                &dir.path().join("src/new")
            ),
            vec![(
                "@game/ReplicatedStorage/Old/Util".to_string(),
                "@game/ReplicatedStorage/New/Util".to_string()
            )]
        );
    }

    #[test]
    fn duplicate_bare_names_are_rejected_without_writing() {
        let dir = make_temp_tree(&[
            ("src/consumer.luau", "return require(\"Config\")\n"),
            ("src/client/Config.luau", "return {}\n"),
            ("src/server/Config.luau", "return {}\n"),
        ]);
        let aliases = HashMap::new();
        let context = context(
            dir.path(),
            &aliases,
            &[
                (
                    dir.path().join("src/client/Config.luau"),
                    "@game/ReplicatedStorage/Client/Config",
                ),
                (
                    dir.path().join("src/server/Config.luau"),
                    "@game/ServerScriptService/Config",
                ),
            ],
        );

        let error = fix_requires_with_context(&context).expect_err("ambiguous bare name must fail");
        assert!(error.to_string().contains("src/client/Config.luau"));
        assert!(error.to_string().contains("src/server/Config.luau"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("src/consumer.luau")).expect("read consumer"),
            "return require(\"Config\")\n"
        );
    }
}

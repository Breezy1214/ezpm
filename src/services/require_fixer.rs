use anyhow::{Context, Result};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use walkdir::WalkDir;

use crate::services::rojo_project::is_within;
use crate::services::sourcemap::{is_wally_index_path, SourcemapIndex};

#[derive(Debug, Default)]
pub struct FixResult {
    pub files_changed: usize,
    pub total_files_scanned: usize,
    pub changes: Vec<FileChange>,
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub file: PathBuf,
    pub rewrites: Vec<RequireRewrite>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequireRewrite {
    pub old: String,
    pub new: String,
    /// 1-based line of the `require` in the original file.
    pub line: usize,
}

/// Turns ascending byte offsets into 1-based line numbers, scanning each byte once.
pub struct LineCounter<'a> {
    content: &'a str,
    offset: usize,
    line: usize,
}

impl<'a> LineCounter<'a> {
    pub fn new(content: &'a str) -> Self {
        Self {
            content,
            offset: 0,
            line: 1,
        }
    }

    /// The line `offset` is on; offsets must not decrease between calls.
    pub fn line_at(&mut self, offset: usize) -> usize {
        self.line += self.content[self.offset..offset].matches('\n').count();
        self.offset = offset;
        self.line
    }
}

#[derive(Clone)]
pub struct FixContext {
    alias_paths: HashMap<String, String>,
    /// Only these may be repaired by module name: redirecting e.g. `@Packages/Signal` to a
    /// local `Signal` module would hijack the require.
    managed_aliases: HashSet<String>,
    project_root: PathBuf,
    source_root: String,
    sourcemap: Arc<SourcemapIndex>,
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
        let public_matches = matches
            .iter()
            .filter_map(|index| self.files.get(*index))
            .filter(|path| !is_wally_index_path(path))
            .collect::<Vec<_>>();

        let candidates = if public_matches.is_empty() {
            matches
                .iter()
                .filter_map(|index| self.files.get(*index))
                .collect::<Vec<_>>()
        } else {
            public_matches
        };

        if candidates.len() == 1 {
            return candidates.first().map(|path| path.as_path());
        }

        if let Ok(mut ambiguities) = self.ambiguities.lock() {
            ambiguities.insert(name.to_string(), candidates.into_iter().cloned().collect());
        }
        None
    }

    fn take_ambiguities(&self) -> Vec<Ambiguity> {
        let Ok(mut stored) = self.ambiguities.lock() else {
            return Vec::new();
        };
        let mut ambiguities = std::mem::take(&mut *stored)
            .into_iter()
            .map(|(name, mut candidates)| {
                candidates.sort();
                Ambiguity { name, candidates }
            })
            .collect::<Vec<_>>();
        ambiguities.sort_by(|left, right| left.name.cmp(&right.name));
        ambiguities
    }
}

fn ambiguity_error(ambiguities: &[Ambiguity]) -> Option<anyhow::Error> {
    if ambiguities.is_empty() {
        return None;
    }
    let details = ambiguities
        .iter()
        .map(|ambiguity| {
            let paths = ambiguity
                .candidates
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            format!(
                "require(\"{}\") matches {}",
                ambiguity.name,
                paths.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Some(anyhow::anyhow!(
        "ambiguous module name; use an alias or source path: {details}"
    ))
}

impl FixContext {
    pub fn new(
        project_root: &Path,
        source_root: &str,
        aliases: &HashMap<String, String>,
        sourcemap: impl Into<Arc<SourcemapIndex>>,
    ) -> Self {
        let sourcemap = sourcemap.into();
        let alias_paths: HashMap<String, String> = aliases
            .iter()
            .map(|(name, path)| (format!("@{name}"), normalize_separators(path)))
            .collect();
        let module_files = sourcemap.source_files();
        let managed_aliases = alias_paths
            .iter()
            .filter(|(_, path)| {
                let dir = project_root.join(path.trim_end_matches('/'));
                let first_at_or_after = module_files.partition_point(|file| *file < dir);
                is_within(path, source_root)
                    || module_files
                        .get(first_at_or_after)
                        .is_some_and(|file| file.starts_with(&dir))
            })
            .map(|(alias, _)| alias.clone())
            .collect();
        Self {
            alias_paths,
            managed_aliases,
            project_root: project_root.to_path_buf(),
            source_root: source_root.to_string(),
            sourcemap,
        }
    }

    pub fn with_aliases(&self, aliases: &HashMap<String, String>) -> Self {
        Self::new(
            &self.project_root,
            &self.source_root,
            aliases,
            Arc::clone(&self.sourcemap),
        )
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

/// A bare `require("name")` that matches several modules, so nothing can be rewritten safely.
#[derive(Debug, Clone, PartialEq)]
pub struct Ambiguity {
    pub name: String,
    pub candidates: Vec<PathBuf>,
}

/// Every rewrite `fix-requires` would make, computed without writing anything.
#[derive(Debug, Default)]
pub struct FixPlan {
    pub total_files_scanned: usize,
    pub pending: Vec<(FileChange, String)>,
    pub ambiguities: Vec<Ambiguity>,
}

pub fn plan_fixes(ctx: &FixContext) -> Result<FixPlan> {
    let files = ctx.script_files();
    let module_index = ctx.module_index();

    let mut pending = Vec::new();
    for path in files {
        let content = read_source(path)?;
        let (new_content, rewrites) =
            process_file_content_with_index(&content, ctx, Some(&module_index));
        let Some(new_content) = new_content else {
            continue;
        };
        pending.push((
            FileChange {
                file: path.clone(),
                rewrites,
            },
            new_content,
        ));
    }

    Ok(FixPlan {
        total_files_scanned: files.len(),
        pending,
        ambiguities: module_index.take_ambiguities(),
    })
}

pub fn fix_requires_with_context(ctx: &FixContext) -> Result<FixResult> {
    let plan = plan_fixes(ctx)?;
    if let Some(error) = ambiguity_error(&plan.ambiguities) {
        return Err(error);
    }
    apply_plan(plan)
}

/// Writes every pending rewrite of a plan that has no ambiguities.
pub fn apply_plan(plan: FixPlan) -> Result<FixResult> {
    let mut changes = Vec::with_capacity(plan.pending.len());
    for (change, content) in plan.pending {
        write_source(&change.file, &content)?;
        changes.push(change);
    }

    Ok(FixResult {
        files_changed: changes.len(),
        total_files_scanned: plan.total_files_scanned,
        changes,
    })
}

pub fn fix_single_file_with_index(
    file_path: &Path,
    ctx: &FixContext,
    module_index: &ModuleIndex,
) -> Result<Option<FileChange>> {
    let content = read_source(file_path)?;
    let (new_content, rewrites) =
        process_file_content_with_index(&content, ctx, Some(module_index));

    if let Some(error) = ambiguity_error(&module_index.take_ambiguities()) {
        return Err(error);
    }

    let Some(new_content) = new_content else {
        return Ok(None);
    };

    write_source(file_path, &new_content)?;
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
        let content = read_source(path)?;
        let rewrites = require_matches(&content)
            .filter_map(|found| {
                let (old_path, new_path) = matching_prefix(found.path, &replacements)?;
                let suffix = &found.path[old_path.len()..];
                Some((found.span, format!("require(\"{new_path}{suffix}\")")))
            })
            .collect::<Vec<_>>();
        if !rewrites.is_empty() {
            let mut updated = content;
            for (span, replacement) in rewrites.into_iter().rev() {
                updated.replace_range(span, &replacement);
            }
            write_source(path, &updated)?;
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

fn read_source(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))
}

/// Source files are rewritten in place rather than via rename: the watcher in `serve`
/// would otherwise see every fix as a temp-file-to-module rename.
fn write_source(path: &Path, contents: &str) -> Result<()> {
    std::fs::write(path, contents).with_context(|| format!("Failed to write {}", path.display()))
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

pub struct RequireMatch<'a> {
    pub span: Range<usize>,
    pub path: &'a str,
}

pub fn require_matches(content: &str) -> impl Iterator<Item = RequireMatch<'_>> {
    static REQUIRE_RE: OnceLock<Regex> = OnceLock::new();
    let re = REQUIRE_RE
        .get_or_init(|| Regex::new(r#"\brequire\("([^"]+)"\)"#).expect("require regex is valid"));
    let comments = if content.contains("require(\"") {
        comment_ranges(content)
    } else {
        Vec::new()
    };
    re.captures_iter(content).filter_map(move |caps| {
        let whole = caps.get(0)?;
        let in_comment = comments
            .get(comments.partition_point(|comment| comment.end <= whole.start()))
            .is_some_and(|comment| comment.start <= whole.start());
        (!in_comment).then(|| RequireMatch {
            span: whole.range(),
            path: caps.get(1).map_or("", |path| path.as_str()),
        })
    })
}

fn comment_ranges(content: &str) -> Vec<Range<usize>> {
    let bytes = content.as_bytes();
    let mut comments = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            quote @ (b'"' | b'\'' | b'`') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote && bytes[i] != b'\n' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            b'[' => match long_bracket_level(bytes, i) {
                Some(level) => i = long_bracket_end(bytes, i + level + 2, level),
                None => i += 1,
            },
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                let start = i;
                i = match long_bracket_level(bytes, i + 2) {
                    Some(level) => long_bracket_end(bytes, i + level + 4, level),
                    None => bytes[i..]
                        .iter()
                        .position(|&b| b == b'\n')
                        .map_or(bytes.len(), |offset| i + offset),
                };
                comments.push(start..i);
            }
            _ => i += 1,
        }
    }
    comments
}

fn long_bracket_level(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let level = bytes[start + 1..]
        .iter()
        .take_while(|&&b| b == b'=')
        .count();
    (bytes.get(start + 1 + level) == Some(&b'[')).then_some(level)
}

fn long_bracket_end(bytes: &[u8], mut i: usize, level: usize) -> usize {
    while i < bytes.len() {
        if bytes[i] == b']'
            && bytes[i + 1..].iter().take(level).all(|&b| b == b'=')
            && bytes.get(i + 1 + level) == Some(&b']')
        {
            return i + level + 2;
        }
        i += 1;
    }
    bytes.len()
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
        let alias = required_path
            .split_once('/')
            .map_or(required_path, |(alias, _)| alias);
        if !ctx.managed_aliases.contains(alias) {
            return None;
        }
    } else if required_path.starts_with('@') {
        return None;
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
    let mut rewrites: Vec<RequireRewrite> = Vec::new();
    let mut rebuilt: Option<String> = None;
    let mut last_end = 0usize;
    let mut lines = LineCounter::new(content);

    for found in require_matches(content) {
        if let Some(new_path) = rewrite_require_path(found.path, ctx, module_index) {
            let output = rebuilt.get_or_insert_with(|| String::with_capacity(content.len()));
            output.push_str(&content[last_end..found.span.start]);
            output.push_str("require(\"");
            output.push_str(&new_path);
            output.push_str("\")");
            rewrites.push(RequireRewrite {
                old: found.path.to_string(),
                new: new_path,
                line: lines.line_at(found.span.start),
            });
            last_end = found.span.end;
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
        let mut sourcemap = SourcemapIndex::from_pairs(pairs);
        sourcemap.add_script_files(&lua_files(project_root));
        FixContext::new(project_root, "src", aliases, sourcemap)
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
    fn unmanaged_alias_requires_are_not_redirected_by_module_name() {
        let aliases = standard_aliases();
        let dir = make_temp_tree(&[("src/shared/Signal.luau", "return {}\n")]);
        let source = dir.path().join("src/shared/Signal.luau");
        let context = context(
            dir.path(),
            &aliases,
            &[(source, "@game/ReplicatedStorage/Shared/Signal")],
        );

        let content =
            "local a = require(\"@Packages/Signal\")\nlocal b = require(\"@lune/Signal\")\n";
        let (new_content, rewrites) = process_file_content(content, &context, Some(dir.path()));

        assert!(rewrites.is_empty(), "unexpected rewrites: {rewrites:?}");
        assert_eq!(new_content, content);
    }

    #[test]
    fn commented_out_and_lookalike_requires_are_ignored() {
        let aliases = standard_aliases();
        let dir = make_temp_tree(&[("src/shared/Util.luau", "return {}\n")]);
        let source = dir.path().join("src/shared/Util.luau");
        let context = context(
            dir.path(),
            &aliases,
            &[(source, "@game/ReplicatedStorage/Shared/Util")],
        );

        let content = concat!(
            "-- local old = require(\"src/shared/Util\")\n",
            "--[==[ require(\"src/shared/Util\") ]==]\n",
            "local custom = myrequire(\"src/shared/Util\")\n",
            "local url = \"http://x\" local m = require(\"src/shared/Util\") -- é\n",
        );
        let (new_content, rewrites) = process_file_content(content, &context, None);

        assert_eq!(rewrites.len(), 1);
        assert!(new_content.starts_with("-- local old = require(\"src/shared/Util\")\n"));
        assert!(new_content.contains("myrequire(\"src/shared/Util\")"));
        assert!(new_content
            .contains("local m = require(\"@game/ReplicatedStorage/Shared/Util\") -- é\n"));
    }

    #[test]
    fn comment_ranges_skip_strings_and_cover_long_comments() {
        let content = "local s = \"--not\" -- gone\nlocal t = [[--kept]] --[[x\ny]] z";
        let ranges = comment_ranges(content);

        let comments: Vec<&str> = ranges.iter().map(|r| &content[r.clone()]).collect();
        assert_eq!(comments, vec!["-- gone", "--[[x\ny]]"]);
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

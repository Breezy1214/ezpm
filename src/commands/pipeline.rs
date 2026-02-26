//! Shared build pipeline

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Context;

use crate::{
    output,
    services::{
        darklua_runner,
        file_watcher::FileChange,
        meta_copier, require_fixer, sourcemap,
    },
};

// ─── build.project.json generation ────────────────────────────────────────────
pub(crate) fn generate_build_project(src: &str, build: &str) -> anyhow::Result<()> {
    let content = std::fs::read_to_string("default.project.json")
        .context("Missing default.project.json — run 'ezpm init' to create it")?;
    let output = content.replace(&format!("{src}/"), &format!("{build}/"));
    std::fs::write("build.project.json", output)
        .context("Failed to write build.project.json")?;
    Ok(())
}

// ─── Path helpers ─────────────────────────────────────────────────────────────

pub(crate) fn src_to_build_path(
    src_file: &Path,
    src_root: &Path,
    build_root: &Path,
) -> Option<PathBuf> {
    src_file
        .strip_prefix(src_root)
        .ok()
        .map(|rel| build_root.join(rel))
}

pub(crate) fn path_for_darklua(path: &Path, project_dir: &Path) -> PathBuf {
    path.strip_prefix(project_dir)
        .unwrap_or(path)
        .to_path_buf()
}

/// Build a user-friendly display name for a file path.
pub(crate) fn display_name(path: &Path) -> String {
    let file_name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();

    // Check if this is an init.* file that needs parent context.
    if file_name.starts_with("init.") {
        if let Some(parent) = path.parent().and_then(|p| p.file_name()) {
            return format!("{}/{}", parent.to_string_lossy(), file_name);
        }
    }

    file_name.into_owned()
}

// ─── Startup steps 1-6 ──────────────────────────────────────────────────────

/// Run startup steps 1-6 (shared by serve and azul).
///
/// 1. Generate build.project.json
/// 2. Clean build directory
/// 3. Generate sourcemap
/// 4. Fix require paths
/// 5. Run DarkLua (with retry)
/// 6. Copy meta files
pub async fn run_startup_steps(
    src: &str,
    build: &str,
    aliases: &HashMap<String, String>,
    src_path: &Path,
    build_path: &Path,
    project_dir: &Path,
) -> anyhow::Result<()> {
    // ── Step 1: Generate build.project.json ──────────────────────────────────
    {
        let pb = output::start_spinner("Generating build.project.json...");
        let t0 = Instant::now();
        let result = generate_build_project(src, build);
        pb.finish_and_clear();
        match result {
            Ok(()) => output::success(&format!(
                "Build project generated ({:.0}ms)",
                t0.elapsed().as_millis()
            )),
            Err(e) => {
                output::error(&format!("Generate build.project.json failed: {}", e));
                return Err(e);
            }
        }
    }

    // ── Step 2: Clean build directory ────────────────────────────────────────
    {
        let pb = output::start_spinner("Cleaning build directory...");
        let t0 = Instant::now();
        let build_path_clone = build_path.to_path_buf();
        let result = darklua_runner::clean_build_dir(&build_path_clone);
        pb.finish_and_clear();
        match result {
            Ok(()) => output::success(&format!(
                "Build directory cleaned ({:.0}ms)",
                t0.elapsed().as_millis()
            )),
            Err(e) => {
                output::error(&format!("Clean build directory failed: {}", e));
                return Err(e);
            }
        }
    }

    // ── Step 3: Generate sourcemap ────────────────────────────────────────────
    {
        let pb = output::start_spinner("Generating sourcemap...");
        let t0 = Instant::now();
        let project_dir_clone = project_dir.to_path_buf();
        let result =
            tokio::task::spawn_blocking(move || sourcemap::generate_sourcemap(&project_dir_clone))
                .await
                .context("sourcemap task panicked")?;
        pb.finish_and_clear();
        match result {
            Ok(r) if r.success => output::success(&format!(
                "Sourcemap generated ({:.0}ms)",
                t0.elapsed().as_millis()
            )),
            Ok(r) => {
                let detail = r.stderr.trim().to_string();
                output::error(&format!("Generate sourcemap failed: {}", detail));
                return Err(anyhow::anyhow!("sourcemap generation failed"));
            }
            Err(e) => {
                output::error(&format!("Generate sourcemap failed: {}", e));
                return Err(e);
            }
        }
    }

    // ── Step 4: Fix require paths ─────────────────────────────────────────────
    {
        let pb = output::start_spinner("Fixing require paths...");
        let t0 = Instant::now();
        let aliases_clone = aliases.clone();
        let src_clone = src.to_string();
        let result = tokio::task::spawn_blocking(move || {
            require_fixer::fix_requires(&PathBuf::from(&src_clone), &aliases_clone, &src_clone)
        })
        .await
        .context("require-fixer task panicked")?;
        pb.finish_and_clear();
        match result {
            Ok(fix_result) => output::success(&format!(
                "Requires fixed ({} files, {:.0}ms)",
                fix_result.files_changed,
                t0.elapsed().as_millis()
            )),
            Err(e) => {
                output::error(&format!("Fix require paths failed: {}", e));
                return Err(e);
            }
        }
    }

    // ── Step 5: Run DarkLua (with retry on warnings) ─────────────────────────
    {
        let pb = output::start_spinner("Running DarkLua...");
        let t0 = Instant::now();
        let src_path_clone = PathBuf::from(src);
        let build_path_clone = PathBuf::from(build);
        let result = tokio::task::spawn_blocking(move || {
            darklua_runner::process_tree_with_retry(&src_path_clone, &build_path_clone)
        })
        .await
        .context("darklua task panicked")?;
        pb.finish_and_clear();
        match result {
            Ok(r) if r.success && r.stderr.trim().is_empty() => output::success(&format!(
                "DarkLua processed ({:.0}ms)",
                t0.elapsed().as_millis()
            )),
            Ok(r) if r.success => {
                let detail = r.stderr.trim().to_string();
                output::error(&format!("DarkLua warnings persist after retry: {}", detail));
                return Err(anyhow::anyhow!("darklua processing had persistent warnings"));
            }
            Ok(r) => {
                let detail = r.stderr.trim().to_string();
                output::error(&format!("DarkLua failed: {}", detail));
                return Err(anyhow::anyhow!("darklua processing failed"));
            }
            Err(e) => {
                output::error(&format!("DarkLua failed: {}", e));
                return Err(e);
            }
        }
    }

    // ── Step 6: Copy meta files ───────────────────────────────────────────────
    {
        let pb = output::start_spinner("Copying meta files...");
        let t0 = Instant::now();
        let src_path_clone = src_path.to_path_buf();
        let build_path_clone = build_path.to_path_buf();
        let result = tokio::task::spawn_blocking(move || {
            meta_copier::copy_meta_files(&src_path_clone, &build_path_clone)
        })
        .await
        .context("meta-copier task panicked")?;
        pb.finish_and_clear();
        match result {
            Ok(count) => output::success(&format!(
                "Meta files copied ({} files, {:.0}ms)",
                count,
                t0.elapsed().as_millis()
            )),
            Err(e) => {
                output::error(&format!("Copy meta files failed: {}", e));
                return Err(e);
            }
        }
    }

    Ok(())
}

// ─── Watch loop change handlers ─────────────────────────────────────────────

pub(crate) async fn handle_changes(
    changes: &[FileChange],
    src: &str,
    build: &str,
    aliases: &HashMap<String, String>,
    project_dir: &Path,
    failed_files: &mut HashSet<PathBuf>,
    file_changes_enabled: bool,
) {
    let t0 = Instant::now();
    let is_batch = changes.len() > 1;

    for change in changes {
        match change {
            FileChange::LuaChange(path) => {
                handle_lua_change(
                    path,
                    src,
                    build,
                    aliases,
                    project_dir,
                    failed_files,
                    is_batch,
                    file_changes_enabled,
                )
                .await;
            }
            FileChange::MetaChange(path) => {
                handle_meta_change(path, src, build, is_batch, file_changes_enabled).await;
            }
            FileChange::FileCreated(path) => {
                handle_file_created(
                    path,
                    src,
                    build,
                    aliases,
                    project_dir,
                    failed_files,
                    is_batch,
                    file_changes_enabled,
                )
                .await;
            }
            FileChange::FileDeleted(path) => {
                handle_file_deleted(path, src, build, project_dir, is_batch, file_changes_enabled)
                    .await;
            }
            FileChange::DirectoryCreated(path) => {
                handle_directory_created(
                    path,
                    src,
                    build,
                    aliases,
                    project_dir,
                    is_batch,
                    file_changes_enabled,
                )
                .await;
            }
            FileChange::DirectoryRemoved(path) => {
                handle_directory_removed(path, src, build, project_dir, is_batch, file_changes_enabled)
                    .await;
            }
        }
    }

    if is_batch && file_changes_enabled {
        output::success(&format!(
            "Rebuilt {} files ({}ms)",
            changes.len(),
            t0.elapsed().as_millis()
        ));
    }
}

/// Handle a `FileChange::LuaChange` event — fix requires then run DarkLua.
async fn handle_lua_change(
    path: &Path,
    src: &str,
    build: &str,
    aliases: &HashMap<String, String>,
    project_dir: &Path,
    failed_files: &mut HashSet<PathBuf>,
    is_batch: bool,
    file_changes_enabled: bool,
) {
    let t0 = Instant::now();

    if let Err(e) = require_fixer::fix_single_file(path, aliases, src) {
        output::error(&format!(
            "{}: require fix failed: {}",
            display_name(path),
            e
        ));
        failed_files.insert(path.to_path_buf());
        return;
    }

    let src_root = project_dir.join(src);
    let build_root = project_dir.join(build);
    let src_parent = match path.parent() {
        Some(p) => p.to_path_buf(),
        None => {
            output::error(&format!(
                "{}: could not determine parent directory",
                display_name(path)
            ));
            failed_files.insert(path.to_path_buf());
            return;
        }
    };
    let build_parent = match src_to_build_path(&src_parent, &src_root, &build_root) {
        Some(p) => p,
        None => {
            output::error(&format!(
                "{}: could not compute build path",
                display_name(path)
            ));
            failed_files.insert(path.to_path_buf());
            return;
        }
    };

    if let Err(e) = std::fs::create_dir_all(&build_parent) {
        output::error(&format!(
            "{}: failed to create build directory: {}",
            display_name(path),
            e
        ));
        failed_files.insert(path.to_path_buf());
        return;
    }

    let darklua_src_parent = path_for_darklua(&src_parent, project_dir);
    let darklua_build_parent = path_for_darklua(&build_parent, project_dir);

    let result = tokio::task::spawn_blocking(move || {
        darklua_runner::process_tree(&darklua_src_parent, &darklua_build_parent)
    })
    .await;

    let filename = display_name(path);

    match result {
        Ok(Ok(r)) if r.success => {
            let was_failed = failed_files.remove(path);
            if !is_batch && file_changes_enabled {
                if was_failed {
                    output::success(&format!("{} fixed ({}ms)", filename, t0.elapsed().as_millis()));
                } else {
                    output::success(&format!("Rebuilt {} ({}ms)", filename, t0.elapsed().as_millis()));
                }
            }
        }
        Ok(Ok(r)) => {
            failed_files.insert(path.to_path_buf());
            let detail = r.stderr.trim().to_string();
            output::error(&format!("{}: {}", filename, detail));
        }
        Ok(Err(e)) => {
            failed_files.insert(path.to_path_buf());
            output::error(&format!("{}: {}", filename, e));
        }
        Err(e) => {
            failed_files.insert(path.to_path_buf());
            output::error(&format!("{}: task panicked: {}", filename, e));
        }
    }
}

/// Handle a `FileChange::MetaChange` event — copy the single meta file to build.
async fn handle_meta_change(
    path: &Path,
    src: &str,
    build: &str,
    is_batch: bool,
    file_changes_enabled: bool,
) {
    let project_dir = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            output::error(&format!("could not determine current directory: {}", e));
            return;
        }
    };

    let src_root = project_dir.join(src);
    let build_root = project_dir.join(build);

    let build_dest = match src_to_build_path(path, &src_root, &build_root) {
        Some(p) => p,
        None => {
            output::error(&format!(
                "{}: could not compute build path for meta file",
                display_name(path)
            ));
            return;
        }
    };

    if let Some(parent) = build_dest.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            output::error(&format!("failed to create build dir for meta file: {}", e));
            return;
        }
    }

    let filename = display_name(path);

    match std::fs::copy(path, &build_dest) {
        Ok(_) => {
            if !is_batch && file_changes_enabled {
                output::success(&format!("Copied {}", filename));
            }
        }
        Err(e) => {
            output::error(&format!("{}: copy failed: {}", filename, e));
        }
    }
}

/// Handle a `FileChange::FileCreated` event.
async fn handle_file_created(
    path: &Path,
    src: &str,
    build: &str,
    aliases: &HashMap<String, String>,
    project_dir: &Path,
    failed_files: &mut HashSet<PathBuf>,
    is_batch: bool,
    file_changes_enabled: bool,
) {
    let t0 = Instant::now();

    let is_lua = matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("lua") | Some("luau")
    );

    if is_lua {
        if let Err(e) = require_fixer::fix_single_file(path, aliases, src) {
            output::error(&format!(
                "{}: require fix failed: {}",
                display_name(path),
                e
            ));
            failed_files.insert(path.to_path_buf());
            return;
        }
    }

    let project_dir_clone = project_dir.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        sourcemap::generate_sourcemap(&project_dir_clone)
    })
    .await;

    match result {
        Ok(Ok(r)) if r.success => {
            if !is_batch && file_changes_enabled {
                output::success(&format!("Sourcemap updated ({}ms)", t0.elapsed().as_millis()));
            }
        }
        Ok(Ok(r)) => {
            let detail = r.stderr.trim().to_string();
            output::error(&format!("Sourcemap update failed: {}", detail));
        }
        Ok(Err(e)) => {
            output::error(&format!("Sourcemap update failed: {}", e));
        }
        Err(e) => {
            output::error(&format!("Sourcemap task panicked: {}", e));
        }
    }

    if is_lua {
        let src_root = project_dir.join(src);
        let build_root = project_dir.join(build);

        let build_parent = match path.parent().and_then(|p| src_to_build_path(p, &src_root, &build_root)) {
            Some(p) => p,
            None => {
                output::error(&format!(
                    "{}: could not compute build path",
                    display_name(path)
                ));
                failed_files.insert(path.to_path_buf());
                return;
            }
        };

        if let Err(e) = std::fs::create_dir_all(&build_parent) {
            output::error(&format!("failed to create build dir: {}", e));
            failed_files.insert(path.to_path_buf());
            return;
        }

        let src_file = path_for_darklua(path, project_dir);
        let darklua_build_parent = path_for_darklua(&build_parent, project_dir);
        let result = tokio::task::spawn_blocking(move || {
            darklua_runner::process_file(&src_file, &darklua_build_parent)
        })
        .await;

        let filename = display_name(path);
        match result {
            Ok(Ok(r)) if r.success => {
                failed_files.remove(path);
                if !is_batch && file_changes_enabled {
                    output::success(&format!("Rebuilt {} ({}ms)", filename, t0.elapsed().as_millis()));
                }
            }
            Ok(Ok(r)) => {
                failed_files.insert(path.to_path_buf());
                let detail = r.stderr.trim().to_string();
                output::error(&format!("{}: {}", filename, detail));
            }
            Ok(Err(e)) => {
                failed_files.insert(path.to_path_buf());
                output::error(&format!("{}: {}", filename, e));
            }
            Err(e) => {
                failed_files.insert(path.to_path_buf());
                output::error(&format!("{}: task panicked: {}", filename, e));
            }
        }
    }
}

/// Handle a `FileChange::DirectoryCreated` event.
async fn handle_directory_created(
    path: &Path,
    src: &str,
    build: &str,
    aliases: &HashMap<String, String>,
    project_dir: &Path,
    is_batch: bool,
    file_changes_enabled: bool,
) {
    let t0 = Instant::now();
    let dirname = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let src_clone = src.to_string();
    let aliases_clone = aliases.clone();
    let result = tokio::task::spawn_blocking(move || {
        require_fixer::fix_requires(&PathBuf::from(&src_clone), &aliases_clone, &src_clone)
    })
    .await;

    match result {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            output::error(&format!("{}/: require fix failed: {}", dirname, e));
            return;
        }
        Err(e) => {
            output::error(&format!("{}/: require fix task panicked: {}", dirname, e));
            return;
        }
    }

    let project_dir_clone = project_dir.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        sourcemap::generate_sourcemap(&project_dir_clone)
    })
    .await;

    match result {
        Ok(Ok(r)) if r.success => {}
        Ok(Ok(r)) => {
            let detail = r.stderr.trim().to_string();
            output::error(&format!("Sourcemap update failed: {}", detail));
        }
        Ok(Err(e)) => {
            output::error(&format!("Sourcemap update failed: {}", e));
        }
        Err(e) => {
            output::error(&format!("Sourcemap task panicked: {}", e));
        }
    }

    let src_root = project_dir.join(src);
    let build_root = project_dir.join(build);

    let build_dir = match src_to_build_path(path, &src_root, &build_root) {
        Some(p) => p,
        None => {
            output::error(&format!(
                "{}/: could not compute build path",
                dirname
            ));
            return;
        }
    };

    if let Err(e) = std::fs::create_dir_all(&build_dir) {
        output::error(&format!("failed to create build dir: {}", e));
        return;
    }

    let src_dir = path_for_darklua(path, project_dir);
    let darklua_build_dir = path_for_darklua(&build_dir, project_dir);
    let result = tokio::task::spawn_blocking(move || {
        darklua_runner::process_tree(&src_dir, &darklua_build_dir)
    })
    .await;

    match result {
        Ok(Ok(r)) if r.success => {
            if !is_batch && file_changes_enabled {
                output::success(&format!("Rebuilt {}/ ({}ms)", dirname, t0.elapsed().as_millis()));
            }
        }
        Ok(Ok(r)) => {
            let detail = r.stderr.trim().to_string();
            output::error(&format!("{}/: darklua failed: {}", dirname, detail));
        }
        Ok(Err(e)) => {
            output::error(&format!("{}/: darklua failed: {}", dirname, e));
        }
        Err(e) => {
            output::error(&format!("{}/: darklua task panicked: {}", dirname, e));
        }
    }
}

/// Handle a `FileChange::DirectoryRemoved` event.
async fn handle_directory_removed(
    path: &Path,
    src: &str,
    build: &str,
    project_dir: &Path,
    is_batch: bool,
    file_changes_enabled: bool,
) {
    let t0 = Instant::now();
    let dirname = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let project_dir_clone = project_dir.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        sourcemap::generate_sourcemap(&project_dir_clone)
    })
    .await;

    match result {
        Ok(Ok(r)) if r.success => {
            if !is_batch && file_changes_enabled {
                output::success(&format!("Sourcemap updated ({}ms)", t0.elapsed().as_millis()));
            }
        }
        Ok(Ok(r)) => {
            let detail = r.stderr.trim().to_string();
            output::error(&format!("Sourcemap update failed: {}", detail));
        }
        Ok(Err(e)) => {
            output::error(&format!("Sourcemap update failed: {}", e));
        }
        Err(e) => {
            output::error(&format!("Sourcemap task panicked: {}", e));
        }
    }

    let src_root = project_dir.join(src);
    let build_root = project_dir.join(build);

    if let Some(build_dir) = src_to_build_path(path, &src_root, &build_root) {
        if build_dir.is_dir() {
            if let Err(e) = std::fs::remove_dir_all(&build_dir) {
                output::error(&format!("failed to remove build dir {}/: {}", dirname, e));
            }
        }
    }
}

/// Handle a `FileChange::FileDeleted` event.
async fn handle_file_deleted(
    path: &Path,
    src: &str,
    build: &str,
    project_dir: &Path,
    is_batch: bool,
    file_changes_enabled: bool,
) {
    let t0 = Instant::now();

    let src_root = project_dir.join(src);
    let build_root = project_dir.join(build);
    if let Some(build_path) = src_to_build_path(path, &src_root, &build_root) {
        if build_path.is_file() {
            if let Err(e) = std::fs::remove_file(&build_path) {
                output::error(&format!("failed to delete build file: {}", e));
            }
        } else if build_path.is_dir() {
            if let Err(e) = std::fs::remove_dir_all(&build_path) {
                output::error(&format!("failed to delete build directory: {}", e));
            }
        }
    }

    let project_dir_clone = project_dir.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        sourcemap::generate_sourcemap(&project_dir_clone)
    })
    .await;

    match result {
        Ok(Ok(r)) if r.success => {
            if !is_batch && file_changes_enabled {
                output::success(&format!("Sourcemap updated ({}ms)", t0.elapsed().as_millis()));
            }
        }
        Ok(Ok(r)) => {
            let detail = r.stderr.trim().to_string();
            output::error(&format!("Sourcemap update failed: {}", detail));
        }
        Ok(Err(e)) => {
            output::error(&format!("Sourcemap update failed: {}", e));
        }
        Err(e) => {
            output::error(&format!("Sourcemap task panicked: {}", e));
        }
    }
}

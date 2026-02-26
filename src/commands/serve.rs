//! Serve command — 9-step startup sequence, port handling, and full watch loop.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Context;
use owo_colors::OwoColorize;
use owo_colors::Stream;

use crate::{
    config::EzpmConfig,
    output,
    services::{
        darklua_runner, file_watcher::{FileChange, FileWatcher, WatchEvent},
        meta_copier, process_manager::{ProcessEvent, ProcessManager}, require_fixer, sourcemap,
    },
};

// ─── Port helpers ──────────────────────────────────────────────────────────────

/// Check if the given port is available for binding.
///
fn port_is_available(port: u16) -> bool {
    std::net::TcpListener::bind(std::net::SocketAddr::from(([0, 0, 0, 0], port))).is_ok()
}

// ─── build.project.json generation ────────────────────────────────────────────
fn generate_build_project(src: &str, build: &str) -> anyhow::Result<()> {
    let content = std::fs::read_to_string("default.project.json")
        .context("Missing default.project.json — run 'ezpm init' to create it")?;
    let output = content.replace(&format!("{src}/"), &format!("{build}/"));
    std::fs::write("build.project.json", output)
        .context("Failed to write build.project.json")?;
    Ok(())
}

// ─── Watch loop helpers ────────────────────────────────────────────────────────
async fn handle_changes(
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

/// Build a user-friendly display name for a file path.
fn display_name(path: &Path) -> String {
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

    // Step 1: Fix require paths — fast in-process, no spawn_blocking needed.
    if let Err(e) = require_fixer::fix_single_file(path, aliases, src) {
        output::error(&format!(
            "{}: require fix failed: {}",
            display_name(path),
            e
        ));
        failed_files.insert(path.to_path_buf());
        return;
    }

    // Step 2: Compute parent directory paths — match Luau's onFileChanged which
    // runs `darklua process <parent_dir> <build_parent_dir>`.
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

    // Ensure build parent directory exists.
    if let Err(e) = std::fs::create_dir_all(&build_parent) {
        output::error(&format!(
            "{}: failed to create build directory: {}",
            display_name(path),
            e
        ));
        failed_files.insert(path.to_path_buf());
        return;
    }

    // Step 3: Run DarkLua on parent directory via spawn_blocking.
    let darklua_src_parent = path_for_darklua(&src_parent, project_dir);
    let darklua_build_parent = path_for_darklua(&build_parent, project_dir);

    let result = tokio::task::spawn_blocking(move || {
        darklua_runner::process_tree(&darklua_src_parent, &darklua_build_parent)
    })
    .await;

    let filename = display_name(path);

    match result {
        Ok(Ok(r)) if r.success => {
            // Step 4: Recovery detection — was this file previously failed?
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
            // DarkLua exited with non-zero — non-fatal, keep watching.
            failed_files.insert(path.to_path_buf());
            let detail = r.stderr.trim().to_string();
            output::error(&format!("{}: {}", filename, detail));
        }
        Ok(Err(e)) => {
            // spawn_blocking closure returned an error.
            failed_files.insert(path.to_path_buf());
            output::error(&format!("{}: {}", filename, e));
        }
        Err(e) => {
            // spawn_blocking task panicked.
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

    // Ensure parent directories exist.
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
///
/// Matches Luau order: fix requires → sourcemap → darklua (single file to build parent dir).
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

    // If the created file is Lua/Luau, fix requires first (before sourcemap).
    let is_lua = matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("lua") | Some("luau")
    );

    if is_lua {
        // Step 1: Fix require paths on the single file.
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

    // Step 2: Regenerate sourcemap.
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

    // Step 3: Run DarkLua — single file to build parent dir (matches Luau removeLast=true).
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

        // Ensure build parent directory exists.
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
///
/// Matches Luau's onDirectoryCreated: fix all requires → sourcemap → darklua on new dir.
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

    // Step 1: Fix requires on the entire src tree (not just the new dir).
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

    // Step 2: Regenerate sourcemap.
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

    // Step 3: Run DarkLua on the new directory → build equivalent.
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

    // Ensure build directory exists.
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
///
/// Matches Luau's onDirectoryRemoved: sourcemap → remove build directory.
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

    // Step 1: Regenerate sourcemap.
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

    // Step 2: Remove the corresponding build directory.
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

    // Delete the corresponding build entry — may be a file or directory.
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

    // Regenerate sourcemap — can take 100-500ms, must use spawn_blocking.
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

/// Handle a `ProcessEvent` — Rojo auto-restart logic.
async fn handle_process_event(
    event: ProcessEvent,
    pm: &mut ProcessManager,
    restart_count: &mut u32,
    port: u16,
) {
    match event {
        ProcessEvent::Crashed { ref name, .. } if name == "rojo" => {
            if *restart_count < 1 {
                output::warn("Rojo exited unexpectedly — restarting...");
                let port_str = port.to_string();
                if let Err(e) = pm
                    .spawn(
                        "rojo",
                        "rojo",
                        &["serve", "build.project.json", "--port", &port_str],
                    )
                    .await
                {
                    output::error(&format!("Failed to restart Rojo: {}", e));
                }
                *restart_count += 1;
            } else {
                output::error("Rojo crashed again — not restarting. File watching continues.");
            }
        }
        ProcessEvent::Exited { name, code } => {
            output::verbose_line(&format!("Process '{}' exited with code {:?}", name, code));
        }
        _ => {
            // Started events are already logged by ProcessManager via verbose_line.
        }
    }
}

// ─── Entry point ───────────────────────────────────────────────────────────────

/// Run the serve command.
pub async fn run(config: Option<EzpmConfig>, cli_port: Option<u16>) -> anyhow::Result<()> {
    let config = config.unwrap_or_default();

    // Extract config values used throughout startup.
    let src = config
        .paths
        .as_ref()
        .and_then(|p| p.src.as_deref())
        .unwrap_or("src")
        .to_string();
    let build = config
        .paths
        .as_ref()
        .and_then(|p| p.darklua_build.as_deref())
        .unwrap_or("darklua_build")
        .to_string();
    let aliases: HashMap<String, String> = config.aliases.clone().unwrap_or_default();
    let file_changes_enabled = config
        .display
        .as_ref()
        .and_then(|d| d.file_changes)
        .unwrap_or(true);

    // Port resolution: CLI flag > ezpm.toml serve.port > default 34872.
    let port: u16 = cli_port
        .or_else(|| config.serve.as_ref().and_then(|s| s.port))
        .unwrap_or(34872);

    // Port availability check 
    if !port_is_available(port) {
        output::error(&format!(
            "Port {} in use. Try: ezpm serve --port {}",
            port,
            port + 1
        ));
        output::hint("Another Rojo session may still be running");
        return Err(anyhow::anyhow!("port {} already in use", port));
    }

    let project_dir = std::env::current_dir().context("could not determine current directory")?;
    let src_path = project_dir.join(&src);
    let build_path = project_dir.join(&build);

    // ── Step 1: Generate build.project.json ──────────────────────────────────
    {
        let pb = output::start_spinner("Generating build.project.json...");
        let t0 = Instant::now();
        let result = generate_build_project(&src, &build);
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
        let build_path_clone = build_path.clone();
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
        let project_dir_clone = project_dir.clone();
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
        let src_clone = src.clone();
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
        let src_path_clone = PathBuf::from(&src);
        let build_path_clone = PathBuf::from(&build);
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
                // Success but stderr still non-empty after retry — fail
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
        let src_path_clone = src_path.clone();
        let build_path_clone = build_path.clone();
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

    // ── Step 7: Start FileWatcher ─────────────────────────────────────────────
    let (watcher, mut watcher_rx) = {
        let pb = output::start_spinner("Starting file watcher...");
        let t0 = Instant::now();
        let result = FileWatcher::new(&src_path, &[]);
        pb.finish_and_clear();
        match result {
            Ok((watcher, rx)) => {
                output::success(&format!(
                    "File watcher started ({:.0}ms)",
                    t0.elapsed().as_millis()
                ));
                (watcher, rx)
            }
            Err(e) => {
                output::error(&format!("Start file watcher failed: {}", e));
                return Err(e);
            }
        }
    };

    // ── Step 8: Start Rojo ────────────────────────────────────────────────────
    let (mut process_manager, mut process_rx) = {
        let pb = output::start_spinner("Starting Rojo...");
        let t0 = Instant::now();
        let port_str = port.to_string();
        let (mut pm, rx) = ProcessManager::new();
        let result = pm
            .spawn(
                "rojo",
                "rojo",
                &["serve", "build.project.json", "--port", &port_str],
            )
            .await;
        pb.finish_and_clear();
        match result {
            Ok(()) => {
                output::success(&format!("Rojo started ({:.0}ms)", t0.elapsed().as_millis()));
                (pm, rx)
            }
            Err(e) => {
                output::error(&format!("Start Rojo failed: {}", e));
                return Err(e);
            }
        }
    };

    // ── Summary banner ────────────────────────────────────────────────────────
    output::print_line("");
    {
        use std::fmt::Write as _;
        let mut banner = String::new();
        let _ = write!(
            banner,
            "  {}  {}",
            "ezpm serve".if_supports_color(Stream::Stdout, |t| t.bold()),
            "ready".if_supports_color(Stream::Stdout, |t| t.green())
        );
        output::print_line(&banner);
    }
    output::print_line("");
    output::info(&format!("Rojo serving on port {}", port));
    output::info(&format!(
        "Watching {}/ for changes (.lua, .luau, init.meta.json)",
        src
    ));
    output::info("Press Ctrl-C to stop");
    output::print_line("");

    // ── Watch loop ────────────────────────────────────────────────────────────
    let mut failed_files: HashSet<PathBuf> = HashSet::new();
    let mut rojo_restart_count: u32 = 0;

    loop {
        tokio::select! {
            event = watcher_rx.recv() => {
                match event {
                    Some(WatchEvent::Changes(changes)) => {
                        handle_changes(
                            &changes,
                            &src,
                            &build,
                            &aliases,
                            &project_dir,
                            &mut failed_files,
                            file_changes_enabled,
                        )
                        .await;
                    }
                    Some(WatchEvent::Error(msg)) => {
                        output::error(&format!("File watcher error: {}", msg));
                        break;
                    }
                    None => break, // watcher dropped — channel closed
                }
            }
            proc_event = process_rx.recv() => {
                if let Some(evt) = proc_event {
                    handle_process_event(
                        evt,
                        &mut process_manager,
                        &mut rojo_restart_count,
                        port,
                    )
                    .await;
                }
            }
            _ = tokio::signal::ctrl_c() => {
                output::info("Stopping...");
                break;
            }
        }
    }

    // Cleanup after loop exit — kill all processes then release the watcher.
    process_manager.kill_all().await;
    drop(watcher);

    Ok(())
}

// ─── Helpers (exposed for testing) ────────────────────────────────────────────

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

    fn path_for_darklua(path: &Path, project_dir: &Path) -> PathBuf {
        path
            .strip_prefix(project_dir)
            .unwrap_or(path)
            .to_path_buf()
    }

//! Serve command — 8-step startup sequence, port handling, and full watch loop.
//!
//! Executes the full development server startup:
//! 1. Generate build.project.json from default.project.json
//! 2. Clean build directory
//! 3. Generate sourcemap
//! 4. Fix require paths
//! 5. Run DarkLua
//! 6. Copy meta files
//! 7. Start FileWatcher
//! 8. Start Rojo
//!
//! After all steps complete, prints a summary banner and enters the
//! `tokio::select!` watch loop that routes file change events to rebuild
//! handlers, handles Rojo lifecycle events, and exits cleanly on Ctrl-C.

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
        darklua_runner, file_watcher::{FileChange, FileWatcher, WatchEvent}, meta_copier,
        process_manager::{ProcessEvent, ProcessManager}, require_fixer, sourcemap,
    },
};

// ─── Port helpers ──────────────────────────────────────────────────────────────

/// Check if the given port is available for binding.
///
/// Uses `TcpListener::bind` — if it succeeds the port is free. This is the
/// stdlib standard pattern for port availability checking.
fn port_is_available(port: u16) -> bool {
    std::net::TcpListener::bind(std::net::SocketAddr::from(([0, 0, 0, 0], port))).is_ok()
}

// ─── build.project.json generation ────────────────────────────────────────────

/// Generate `build.project.json` from `default.project.json` by substituting
/// the src path with the build path via simple string replacement.
///
/// No JSON parsing needed — this mirrors the Luau `generateBuildProject` which
/// uses `string.gsub` on the raw file content.
///
/// # Errors
///
/// Returns an error if `default.project.json` is missing (user must run
/// `ezpm init` first) or if the output file cannot be written.
fn generate_build_project(src: &str, build: &str) -> anyhow::Result<()> {
    let content = std::fs::read_to_string("default.project.json")
        .context("Missing default.project.json — run 'ezpm init' to create it")?;
    let output = content.replace(&format!("{src}/"), &format!("{build}/"));
    std::fs::write("build.project.json", output)
        .context("Failed to write build.project.json")?;
    Ok(())
}

// ─── Watch loop helpers ────────────────────────────────────────────────────────

/// Handle a batch of file change events.
///
/// Dispatches each `FileChange` to the appropriate rebuild handler and prints
/// either per-file feedback (single change) or a batch summary line (>1 changes).
async fn handle_changes(
    changes: &[FileChange],
    src: &str,
    build: &str,
    aliases: &HashMap<String, String>,
    project_dir: &Path,
    failed_files: &mut HashSet<PathBuf>,
) {
    let t0 = Instant::now();
    let is_batch = changes.len() > 1;

    for change in changes {
        match change {
            FileChange::LuaChange(path) => {
                handle_lua_change(path, src, build, aliases, project_dir, failed_files, is_batch).await;
            }
            FileChange::MetaChange(path) => {
                handle_meta_change(path, src, build, is_batch).await;
            }
            FileChange::FileCreated(path) => {
                handle_file_created(path, src, build, aliases, project_dir, failed_files, is_batch).await;
            }
            FileChange::FileDeleted(path) => {
                handle_file_deleted(path, src, build, project_dir, is_batch).await;
            }
        }
    }

    if is_batch {
        output::success(&format!(
            "Rebuilt {} files ({}ms)",
            changes.len(),
            t0.elapsed().as_millis()
        ));
    }
}

/// Build a user-friendly display name for a file path.
///
/// For `init.*` files, includes the parent directory to disambiguate:
///   `src/MyModule/init.luau` -> `MyModule/init.luau`
///   `src/Services/init.meta.json` -> `Services/init.meta.json`
///
/// For all other files, returns just the filename:
///   `src/MyModule/Foo.luau` -> `Foo.luau`
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
///
/// `fix_single_file` is pure in-process string manipulation (no subprocess),
/// so it can be called directly without `spawn_blocking`.
/// `darklua_runner::process_file` uses `std::process::Command` (blocking),
/// so it MUST be wrapped in `spawn_blocking`.
async fn handle_lua_change(
    path: &Path,
    src: &str,
    build: &str,
    aliases: &HashMap<String, String>,
    project_dir: &Path,
    failed_files: &mut HashSet<PathBuf>,
    is_batch: bool,
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

    // Step 2: Build the corresponding path in build dir.
    let src_root = project_dir.join(src);
    let build_root = project_dir.join(build);
    let build_file = match src_to_build_path(path, &src_root, &build_root) {
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

    // Step 3: Run DarkLua via spawn_blocking (std::process::Command is blocking).
    let src_file = path.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        darklua_runner::process_file(&src_file, &build_file)
    })
    .await;

    let filename = display_name(path);

    match result {
        Ok(Ok(r)) if r.success => {
            // Step 4: Recovery detection — was this file previously failed?
            let was_failed = failed_files.remove(path);
            if !is_batch {
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
async fn handle_meta_change(path: &Path, src: &str, build: &str, is_batch: bool) {
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
            if !is_batch {
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
/// Regenerates the sourcemap and, if the created file is Lua/Luau, also runs
/// the full Lua rebuild pipeline (fix requires + DarkLua).
async fn handle_file_created(
    path: &Path,
    src: &str,
    build: &str,
    aliases: &HashMap<String, String>,
    project_dir: &Path,
    failed_files: &mut HashSet<PathBuf>,
    is_batch: bool,
) {
    let t0 = Instant::now();
    let project_dir_clone = project_dir.to_path_buf();

    // Regenerate sourcemap — can take 100-500ms, must use spawn_blocking.
    let result = tokio::task::spawn_blocking(move || {
        sourcemap::generate_sourcemap(&project_dir_clone)
    })
    .await;

    match result {
        Ok(Ok(r)) if r.success => {
            if !is_batch {
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

    // If the created file is Lua/Luau, also run the full rebuild pipeline.
    let is_lua = matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("lua") | Some("luau")
    );
    if is_lua {
        handle_lua_change(path, src, build, aliases, project_dir, failed_files, is_batch).await;
    }
}

/// Handle a `FileChange::FileDeleted` event.
///
/// Deletes the corresponding file from the build directory (if it exists) and
/// regenerates the sourcemap.
async fn handle_file_deleted(
    path: &Path,
    src: &str,
    build: &str,
    project_dir: &Path,
    is_batch: bool,
) {
    let t0 = Instant::now();

    // Delete the corresponding build file if it exists.
    let src_root = project_dir.join(src);
    let build_root = project_dir.join(build);
    if let Some(build_file) = src_to_build_path(path, &src_root, &build_root) {
        if build_file.exists() {
            if let Err(e) = std::fs::remove_file(&build_file) {
                output::error(&format!("failed to delete build file: {}", e));
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
            if !is_batch {
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
///
/// Rojo auto-restarts once if it crashes. A second crash logs the error
/// without restarting. Other events (Started, Exited) are logged at verbose
/// level since they are expected lifecycle events.
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
///
/// Executes the 8-step startup sequence with per-step spinners and timing,
/// resolves the Rojo port (CLI flag > ezpm.toml > default 34872), checks port
/// availability, then launches Rojo. Prints a summary banner on success.
///
/// After the banner, enters the full `tokio::select!` watch loop that:
/// - Routes file change events to the appropriate rebuild handler
/// - Handles Rojo process lifecycle events (auto-restart on crash)
/// - Exits cleanly on Ctrl-C
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

    // Port resolution: CLI flag > ezpm.toml serve.port > default 34872.
    let port: u16 = cli_port
        .or_else(|| config.serve.as_ref().and_then(|s| s.port))
        .unwrap_or(34872);

    // Port availability check — must happen before any build steps so the user
    // gets immediate feedback rather than waiting through the full startup.
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
        let project_dir_clone = project_dir.clone();
        let aliases_clone = aliases.clone();
        let src_clone = src.clone();
        let result = tokio::task::spawn_blocking(move || {
            require_fixer::fix_requires(&project_dir_clone, &aliases_clone, &src_clone)
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

    // ── Step 5: Run DarkLua ───────────────────────────────────────────────────
    {
        let pb = output::start_spinner("Running DarkLua...");
        let t0 = Instant::now();
        let src_path_clone = src_path.clone();
        let build_path_clone = build_path.clone();
        let result = tokio::task::spawn_blocking(move || {
            darklua_runner::process_tree(&src_path_clone, &build_path_clone)
        })
        .await
        .context("darklua task panicked")?;
        pb.finish_and_clear();
        match result {
            Ok(r) if r.success => output::success(&format!(
                "DarkLua processed ({:.0}ms)",
                t0.elapsed().as_millis()
            )),
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
    // Use owo-colors with if_supports_color. Each colored segment must be
    // formatted to an owned String before combining to avoid borrow conflicts.
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
    // 3-arm tokio::select! loop:
    //   1. watcher_rx — file change events from FileWatcher
    //   2. process_rx — process lifecycle events from ProcessManager
    //   3. ctrl_c    — user interrupt (clean shutdown)
    //
    // Design decisions:
    // - Individual rebuild failures are non-fatal — error printed inline, loop continues.
    // - WatchEvent::Error and ctrl_c are the only loop-exit conditions.
    // - Rojo auto-restarts once on crash; second crash logs but does not restart.
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

/// Get the build-equivalent path for a source file.
///
/// Given a file path under `src_root`, returns the corresponding path under
/// `build_root`. Used by the watch loop rebuild handlers.
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

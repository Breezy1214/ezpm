//! Serve command — 8-step startup sequence, port handling, and Rojo launch.
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
//! After all steps complete, prints a summary banner and waits for Ctrl-C.
//! Plan 02 will replace the Ctrl-C wait with a tokio::select! watch loop.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Context;
use owo_colors::OwoColorize;
use owo_colors::Stream;

use crate::{
    config::EzpmConfig,
    output,
    services::{
        darklua_runner, file_watcher::FileWatcher, meta_copier, process_manager::ProcessManager,
        require_fixer, sourcemap,
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

// ─── Entry point ───────────────────────────────────────────────────────────────

/// Run the serve command.
///
/// Executes the 8-step startup sequence with per-step spinners and timing,
/// resolves the Rojo port (CLI flag > ezpm.toml > default 34872), checks port
/// availability, then launches Rojo. Prints a summary banner on success.
///
/// After the banner, waits for Ctrl-C then gracefully shuts down.
/// Plan 02 will replace this with a full `tokio::select!` watch loop.
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
    let (watcher, watcher_rx) = {
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
    let (mut process_manager, process_rx) = {
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

    // Keep process_rx alive — Plan 02 uses it in the select! loop.
    // Suppress unused variable warning until Plan 02 replaces this block.
    let _ = process_rx;
    // Keep watcher_rx alive — Plan 02 uses it in the select! loop.
    let _ = watcher_rx;

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

    // ── Temporary: wait for Ctrl-C, then clean up ─────────────────────────────
    // Plan 02 replaces this with a tokio::select! watch loop that also handles
    // file change events and Rojo lifecycle events.
    tokio::signal::ctrl_c().await?;
    output::info("Stopping...");
    process_manager.kill_all().await;
    drop(watcher);

    Ok(())
}

// ─── Helpers (exposed for testing) ────────────────────────────────────────────

/// Get the build-equivalent path for a source file.
///
/// Given a file path under `src_root`, returns the corresponding path under
/// `build_root`. Used by the watch loop in Plan 02.
#[allow(dead_code)]
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

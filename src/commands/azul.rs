//! Azul command — two-way sync with DarkLua processing.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::Context;
use owo_colors::OwoColorize;
use owo_colors::Stream;

use crate::{
    config::EzpmConfig,
    output,
    services::{
        file_watcher::{FileWatcher, WatchEvent},
        process_manager::{ProcessEvent, ProcessManager},
    },
};

use super::pipeline;

/// Handle a `ProcessEvent` — Azul auto-restart logic.
async fn handle_process_event(
    event: ProcessEvent,
    pm: &mut ProcessManager,
    restart_count: &mut u32,
    rojo_project: &str,
) {
    match event {
        ProcessEvent::Crashed { ref name, .. } if name == "azul" => {
            if *restart_count < 1 {
                output::warn("Azul exited unexpectedly — restarting...");
                let project_arg = format!("--rojo-project={}", rojo_project);
                if let Err(e) = pm
                    .spawn("azul", "azul", &["--rojo", &project_arg])
                    .await
                {
                    output::error(&format!("Failed to restart Azul: {}", e));
                }
                *restart_count += 1;
            } else {
                output::error("Azul crashed again — not restarting. File watching continues.");
            }
        }
        ProcessEvent::Exited { name, code } => {
            output::verbose_line(&format!("Process '{}' exited with code {:?}", name, code));
        }
        _ => {}
    }
}

// ─── Entry point ───────────────────────────────────────────────────────────────

/// Run the azul command.
pub async fn run(config: Option<EzpmConfig>) -> anyhow::Result<()> {
    let config = config.unwrap_or_default();

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

    // Azul config
    let rojo_compat = config
        .azul
        .as_ref()
        .and_then(|a| a.rojo_compat)
        .unwrap_or(true);
    let rojo_project = config
        .azul
        .as_ref()
        .and_then(|a| a.rojo_project.clone())
        .unwrap_or_else(|| "build.project.json".to_string());

    let project_dir = std::env::current_dir().context("could not determine current directory")?;
    let src_path = project_dir.join(&src);
    let build_path = project_dir.join(&build);

    // ── Steps 1-6: Shared pipeline ──────────────────────────────────────────
    pipeline::run_startup_steps(&src, &build, &aliases, &src_path, &build_path, &project_dir)
        .await?;

    // ── Step 7: Start FileWatcher ─────────────────────────────────────────────
    let (watcher, mut watcher_rx) = {
        let pb = output::start_spinner("Starting file watcher...");
        let t0 = std::time::Instant::now();
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

    // ── Step 8: Start Azul ────────────────────────────────────────────────────
    let (mut process_manager, mut process_rx) = {
        let pb = output::start_spinner("Starting Azul...");
        let t0 = std::time::Instant::now();
        let (mut pm, rx) = ProcessManager::new();

        let mut args: Vec<String> = Vec::new();
        if rojo_compat {
            args.push("--rojo".to_string());
            args.push(format!("--rojo-project={}", rojo_project));
        }

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let result = pm.spawn("azul", "azul", &args_refs).await;
        pb.finish_and_clear();
        match result {
            Ok(()) => {
                output::success(&format!("Azul started ({:.0}ms)", t0.elapsed().as_millis()));
                (pm, rx)
            }
            Err(e) => {
                output::error(&format!("Start Azul failed: {}", e));
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
            "ezpm azul".if_supports_color(Stream::Stdout, |t| t.bold()),
            "ready".if_supports_color(Stream::Stdout, |t| t.green())
        );
        output::print_line(&banner);
    }
    output::print_line("");
    output::info("Azul syncing with Roblox Studio");
    output::info(&format!(
        "Watching {}/ for changes (.lua, .luau, init.meta.json)",
        src
    ));
    output::info("Press Ctrl-C to stop");
    output::print_line("");

    // ── Watch loop ────────────────────────────────────────────────────────────
    let mut failed_files: HashSet<PathBuf> = HashSet::new();
    let mut azul_restart_count: u32 = 0;

    loop {
        tokio::select! {
            event = watcher_rx.recv() => {
                match event {
                    Some(WatchEvent::Changes(changes)) => {
                        pipeline::handle_changes(
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
                    None => break,
                }
            }
            proc_event = process_rx.recv() => {
                if let Some(evt) = proc_event {
                    handle_process_event(
                        evt,
                        &mut process_manager,
                        &mut azul_restart_count,
                        &rojo_project,
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

    process_manager.kill_all().await;
    drop(watcher);

    Ok(())
}

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::{
    config::{self, EzpmConfig, RequireFixMode},
    output,
    services::{
        darklua_runner,
        file_watcher::{FileChange, FileWatcher, WatchEvent, WatchTargets},
        meta_copier,
        process_manager::{ProcessEvent, ProcessManager},
        require_fixer,
        rojo_project::{self, RojoProjectSettings},
        sourcemap, toolchain,
    },
};
use anyhow::Context;
use owo_colors::OwoColorize;
use owo_colors::Stream;

fn port_is_available(port: u16) -> bool {
    std::net::TcpListener::bind(std::net::SocketAddr::from(([0, 0, 0, 0], port))).is_ok()
}

fn sync_toolchain_versions(project_dir: &Path) -> anyhow::Result<()> {
    let rokit_path = project_dir.join("rokit.toml");
    if !rokit_path.exists() {
        return Ok(());
    }

    let contents = match std::fs::read_to_string(&rokit_path) {
        Ok(c) => c,
        Err(e) => {
            output::warn(&format!(
                "Skipping toolchain sync — could not read rokit.toml: {}",
                e
            ));
            return Ok(());
        }
    };

    let updates = match toolchain::outdated_tools(&contents) {
        Ok(u) => u,
        Err(e) => {
            output::warn(&format!("Skipping toolchain sync — {}", e));
            return Ok(());
        }
    };

    if updates.is_empty() {
        return Ok(());
    }

    std::fs::write(
        &rokit_path,
        toolchain::apply_tool_updates(&contents, &updates),
    )
    .context("Failed to write updated rokit.toml")?;

    for u in &updates {
        let from = toolchain::spec_version(&u.old_spec).unwrap_or(u.old_spec.as_str());
        let to = toolchain::spec_version(&u.new_spec).unwrap_or(u.new_spec.as_str());
        output::info(&format!("Updated {} {} \u{2192} {}", u.name, from, to));
    }

    let pb = output::start_spinner("Installing updated tools (rokit install)...");
    let result = std::process::Command::new("rokit").arg("install").output();
    pb.finish_and_clear();

    match result {
        Ok(out) if out.status.success() => {
            output::success(&format!(
                "Toolchain updated ({} {} refreshed)",
                updates.len(),
                if updates.len() == 1 { "tool" } else { "tools" }
            ));
            Ok(())
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            output::error(&format!("rokit install failed: {}", stderr.trim()));
            Err(anyhow::anyhow!(
                "rokit install failed after updating tool versions"
            ))
        }
        Err(e) => {
            output::error(&format!("Failed to run rokit install: {}", e));
            Err(anyhow::anyhow!(
                "{}",
                toolchain::missing_tool_context("rokit")
            ))
        }
    }
}

async fn run_fix_requires(
    src: &str,
    aliases: &HashMap<String, String>,
) -> anyhow::Result<require_fixer::FixResult> {
    let src_clone = src.to_string();
    let aliases_clone = aliases.clone();
    tokio::task::spawn_blocking(move || {
        require_fixer::fix_requires(&PathBuf::from(&src_clone), &aliases_clone, &src_clone)
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!("task panicked: {}", e)))
}

struct ChangeContext<'a> {
    src_prefix: &'a str,
    source_root: &'a Path,
    build_root: &'a Path,
    aliases: &'a HashMap<String, String>,
    require_fix_mode: RequireFixMode,
    project_dir: &'a Path,
    generated_project: &'a Path,
    file_changes_enabled: bool,
    fix_ctx: &'a require_fixer::FixContext,
}

fn needs_full_require_fix(mode: RequireFixMode, changes: &[FileChange]) -> bool {
    match mode {
        RequireFixMode::Strict => changes
            .iter()
            .any(|change| !matches!(change, FileChange::MetaChange(_))),
        RequireFixMode::Hybrid => changes.iter().any(|change| {
            matches!(
                change,
                FileChange::FileDeleted(_) | FileChange::DirectoryRemoved(_)
            )
        }),
        RequireFixMode::Fast => false,
    }
}

fn changes_project_topology(changes: &[FileChange]) -> bool {
    changes.iter().any(|change| {
        matches!(
            change,
            FileChange::FileCreated(_)
                | FileChange::FileDeleted(_)
                | FileChange::DirectoryCreated(_)
                | FileChange::DirectoryRemoved(_)
        )
    })
}

async fn refresh_sourcemap(context: &ChangeContext<'_>) {
    let project_dir = context.project_dir.to_path_buf();
    let generated_project = context.generated_project.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        sourcemap::generate_sourcemap_for_project(&project_dir, &generated_project)
    })
    .await;

    match result {
        Ok(Ok(result)) if result.success => {}
        Ok(Ok(result)) => output::error(&format!(
            "Sourcemap update failed: {}",
            result.stderr.trim()
        )),
        Ok(Err(error)) => output::error(&format!("Sourcemap update failed: {error}")),
        Err(error) => output::error(&format!("Sourcemap task panicked: {error}")),
    }
}

async fn handle_changes(
    changes: &[FileChange],
    context: &ChangeContext<'_>,
    failed_files: &mut HashSet<PathBuf>,
) {
    let t0 = Instant::now();
    let is_batch = changes.len() > 1;

    let full_fix_performed = needs_full_require_fix(context.require_fix_mode, changes);
    if full_fix_performed {
        if let Err(error) = run_fix_requires(context.src_prefix, context.aliases).await {
            output::error(&format!("Require fix failed: {error}"));
            return;
        }
    }

    for change in changes {
        match change {
            FileChange::LuaChange(path) => {
                handle_lua_file(path, context, failed_files, is_batch, full_fix_performed).await;
            }
            FileChange::MetaChange(path) => {
                handle_meta_change(path, context, is_batch).await;
            }
            FileChange::FileCreated(path) => {
                if is_lua_file(path) {
                    handle_lua_file(path, context, failed_files, is_batch, full_fix_performed)
                        .await;
                }
            }
            FileChange::FileDeleted(path) => {
                handle_file_deleted(path, context).await;
            }
            FileChange::DirectoryCreated(path) => {
                handle_directory_created(path, context, is_batch).await;
            }
            FileChange::DirectoryRemoved(path) => {
                handle_directory_removed(path, context).await;
            }
            FileChange::RojoProjectChange(_) | FileChange::ConfigChange(_) => {}
        }
    }

    if changes_project_topology(changes) {
        refresh_sourcemap(context).await;
    }

    if is_batch && context.file_changes_enabled {
        output::success(&format!(
            "Rebuilt {} files ({}ms)",
            changes.len(),
            t0.elapsed().as_millis()
        ));
    }
}

fn display_name(path: &Path) -> String {
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();

    if file_name.starts_with("init.") {
        if let Some(parent) = path.parent().and_then(|p| p.file_name()) {
            return format!("{}/{}", parent.to_string_lossy(), file_name);
        }
    }

    file_name.into_owned()
}

fn is_lua_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("lua") | Some("luau")
    )
}

async fn handle_lua_file(
    path: &Path,
    context: &ChangeContext<'_>,
    failed_files: &mut HashSet<PathBuf>,
    is_batch: bool,
    full_fix_performed: bool,
) {
    let t0 = Instant::now();

    if !full_fix_performed && !matches!(context.require_fix_mode, RequireFixMode::Strict) {
        if let Err(e) = require_fixer::fix_single_file_with_context(path, context.fix_ctx) {
            output::error(&format!(
                "{}: require fix failed: {}",
                display_name(path),
                e
            ));
            failed_files.insert(path.to_path_buf());
            return;
        }
    }

    let build_file = match src_to_build_path(path, context.source_root, context.build_root) {
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

    let Some(build_parent) = build_file.parent() else {
        output::error(&format!(
            "{}: could not determine build directory",
            display_name(path)
        ));
        failed_files.insert(path.to_path_buf());
        return;
    };

    if let Err(e) = std::fs::create_dir_all(build_parent) {
        output::error(&format!(
            "{}: failed to create build directory: {}",
            display_name(path),
            e
        ));
        failed_files.insert(path.to_path_buf());
        return;
    }

    let darklua_source = path_for_darklua(path, context.project_dir);
    let darklua_build_file = path_for_darklua(&build_file, context.project_dir);

    let result = tokio::task::spawn_blocking(move || {
        darklua_runner::process_file(&darklua_source, &darklua_build_file)
    })
    .await;

    let filename = display_name(path);

    match result {
        Ok(Ok(r)) if r.success => {
            let was_failed = failed_files.remove(path);
            if !is_batch && context.file_changes_enabled {
                if was_failed {
                    output::success(&format!(
                        "{} fixed ({}ms)",
                        filename,
                        t0.elapsed().as_millis()
                    ));
                } else {
                    output::success(&format!(
                        "Rebuilt {} ({}ms)",
                        filename,
                        t0.elapsed().as_millis()
                    ));
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

async fn handle_meta_change(path: &Path, context: &ChangeContext<'_>, is_batch: bool) {
    let build_dest = match src_to_build_path(path, context.source_root, context.build_root) {
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
            if !is_batch && context.file_changes_enabled {
                output::success(&format!("Copied {}", filename));
            }
        }
        Err(e) => {
            output::error(&format!("{}: copy failed: {}", filename, e));
        }
    }
}

async fn handle_directory_created(path: &Path, context: &ChangeContext<'_>, is_batch: bool) {
    let t0 = Instant::now();
    let dirname = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let build_dir = match src_to_build_path(path, context.source_root, context.build_root) {
        Some(p) => p,
        None => {
            output::error(&format!("{}/: could not compute build path", dirname));
            return;
        }
    };

    if let Err(e) = std::fs::create_dir_all(&build_dir) {
        output::error(&format!("failed to create build dir: {}", e));
        return;
    }

    let src_dir = path_for_darklua(path, context.project_dir);
    let darklua_build_dir = path_for_darklua(&build_dir, context.project_dir);
    let result = tokio::task::spawn_blocking(move || {
        darklua_runner::process_tree(&src_dir, &darklua_build_dir)
    })
    .await;

    match result {
        Ok(Ok(r)) if r.success => {
            if !is_batch && context.file_changes_enabled {
                output::success(&format!(
                    "Rebuilt {}/ ({}ms)",
                    dirname,
                    t0.elapsed().as_millis()
                ));
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

async fn handle_directory_removed(path: &Path, context: &ChangeContext<'_>) {
    let dirname = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    if let Some(build_dir) = src_to_build_path(path, context.source_root, context.build_root) {
        if build_dir.is_dir() {
            if let Err(e) = std::fs::remove_dir_all(&build_dir) {
                output::error(&format!("failed to remove build dir {}/: {}", dirname, e));
            }
        }
    }
}

async fn handle_file_deleted(path: &Path, context: &ChangeContext<'_>) {
    if let Some(build_path) = src_to_build_path(path, context.source_root, context.build_root) {
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
}

async fn handle_process_event(
    event: ProcessEvent,
    pm: &mut ProcessManager,
    restart_count: &mut u32,
    port: u16,
    generated_project: &Path,
) {
    match event {
        ProcessEvent::Crashed { ref name, .. } if name == "rojo" => {
            if *restart_count < 1 {
                output::warn("Rojo exited unexpectedly — restarting...");
                let port_str = port.to_string();
                let generated_project = generated_project.to_string_lossy();
                if let Err(e) = pm
                    .spawn(
                        "rojo",
                        "rojo",
                        &["serve", &generated_project, "--port", &port_str],
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
        _ => {}
    }
}

fn watch_targets(
    project_dir: &Path,
    source_root: &Path,
    build_root: &Path,
    settings: &RojoProjectSettings,
) -> WatchTargets {
    WatchTargets {
        source_root: source_root.to_path_buf(),
        project_files: vec![project_dir.join(&settings.project)],
        config_file: Some(project_dir.join("ezpm.toml")),
        generated_roots: vec![
            build_root.to_path_buf(),
            project_dir.join(&settings.generated_project),
        ],
    }
}

async fn regenerate_rojo_project(
    project_dir: &Path,
    settings: &RojoProjectSettings,
) -> anyhow::Result<bool> {
    let generation_root = project_dir.to_path_buf();
    let settings = settings.clone();
    let generation = tokio::task::spawn_blocking(move || {
        rojo_project::generate_build_project(&generation_root, &settings)
    })
    .await
    .context("Rojo project generation task panicked")??;

    if !generation.written {
        return Ok(false);
    }

    let project_dir = project_dir.to_path_buf();
    let generated_project = generation.generated_project.clone();
    let sourcemap = tokio::task::spawn_blocking(move || {
        sourcemap::generate_sourcemap_for_project(&project_dir, &generated_project)
    })
    .await
    .context("sourcemap task panicked")??;

    if !sourcemap.success {
        anyhow::bail!("sourcemap generation failed: {}", sourcemap.stderr.trim());
    }

    output::success(&format!(
        "Rojo project regenerated ({} paths remapped)",
        generation.remapped_paths
    ));
    Ok(true)
}

async fn restart_rojo(
    process_manager: &mut ProcessManager,
    process_rx: &mut tokio::sync::mpsc::Receiver<ProcessEvent>,
    port: u16,
    generated_project: &Path,
) -> anyhow::Result<()> {
    process_manager.kill("rojo").await;
    while process_rx.try_recv().is_ok() {}

    let port = port.to_string();
    let generated_project = generated_project.to_string_lossy();
    process_manager
        .spawn(
            "rojo",
            "rojo",
            &["serve", &generated_project, "--port", &port],
        )
        .await
        .context("failed to restart Rojo after project regeneration")
}

pub async fn run(config: Option<EzpmConfig>, cli_port: Option<u16>) -> anyhow::Result<()> {
    let config = config.unwrap_or_default();
    let mut rojo_settings = RojoProjectSettings::from_config(&config);

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
    let require_fix_mode = config
        .serve
        .as_ref()
        .and_then(|s| s.require_fix_mode)
        .unwrap_or_default();

    let port: u16 = cli_port
        .or_else(|| config.serve.as_ref().and_then(|s| s.port))
        .unwrap_or(34872);

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

    sync_toolchain_versions(&project_dir)?;

    {
        let pb = output::start_spinner(&format!(
            "Generating {}...",
            rojo_settings.generated_project.display()
        ));
        let t0 = Instant::now();
        let result = rojo_project::generate_build_project(&project_dir, &rojo_settings);
        pb.finish_and_clear();
        match result {
            Ok(generation) => {
                let status = if generation.written {
                    "generated"
                } else {
                    "already current"
                };
                output::success(&format!(
                    "Build project {} ({} paths remapped, {:.0}ms)",
                    status,
                    generation.remapped_paths,
                    t0.elapsed().as_millis()
                ));
            }
            Err(e) => {
                output::error(&format!("Generate build project failed: {}", e));
                return Err(e);
            }
        }
    }

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

    {
        let pb = output::start_spinner("Generating sourcemap...");
        let t0 = Instant::now();
        let project_dir_clone = project_dir.clone();
        let generated_project = rojo_settings.generated_project.clone();
        let result = tokio::task::spawn_blocking(move || {
            sourcemap::generate_sourcemap_for_project(&project_dir_clone, &generated_project)
        })
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
                let detail = r.stderr.trim().to_string();
                output::error(&format!("DarkLua warnings persist after retry: {}", detail));
                return Err(anyhow::anyhow!(
                    "darklua processing had persistent warnings"
                ));
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

    let (mut watcher, mut watcher_rx) = {
        let pb = output::start_spinner("Starting file watcher...");
        let t0 = Instant::now();
        let result = FileWatcher::with_targets(
            watch_targets(&project_dir, &src_path, &build_path, &rojo_settings),
            &[],
        );
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

    let (mut process_manager, mut process_rx) = {
        let pb = output::start_spinner("Starting Rojo...");
        let t0 = Instant::now();
        let port_str = port.to_string();
        let generated_project = rojo_settings.generated_project.to_string_lossy();
        let (mut pm, rx) = ProcessManager::new();
        let result = pm
            .spawn(
                "rojo",
                "rojo",
                &["serve", &generated_project, "--port", &port_str],
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
        "Watching {}/ for changes (.lua, .luau, init.meta.json, *.model.json)",
        src
    ));
    output::info(&format!("Require-fix mode: {:?}", require_fix_mode).to_lowercase());
    output::info("Press Ctrl-C to stop");
    output::print_line("");

    let mut failed_files: HashSet<PathBuf> = HashSet::new();
    let mut rojo_restart_count: u32 = 0;
    let fix_ctx = require_fixer::FixContext::new(&aliases, &src);

    #[cfg(unix)]
    let mut sig_hup = {
        use tokio::signal::unix::{signal, SignalKind};
        signal(SignalKind::hangup()).context("failed to register SIGHUP handler")?
    };
    #[cfg(unix)]
    let mut sig_term = {
        use tokio::signal::unix::{signal, SignalKind};
        signal(SignalKind::terminate()).context("failed to register SIGTERM handler")?
    };

    loop {
        let terminal_signal = async {
            #[cfg(unix)]
            {
                tokio::select! {
                    _ = sig_hup.recv() => {}
                    _ = sig_term.recv() => {}
                }
            }
            #[cfg(not(unix))]
            std::future::pending::<()>().await
        };

        tokio::select! {
            event = watcher_rx.recv() => {
                match event {
                    Some(WatchEvent::Changes(changes)) => {
                        let config_changed = changes
                            .iter()
                            .any(|change| matches!(change, FileChange::ConfigChange(_)));
                        let project_changed = changes
                            .iter()
                            .any(|change| matches!(change, FileChange::RojoProjectChange(_)));

                        if config_changed {
                            match config::load_config() {
                                Ok((new_config, warnings)) => {
                                    for warning in warnings {
                                        output::warn(&warning);
                                    }
                                    let new_src = new_config
                                        .paths
                                        .as_ref()
                                        .and_then(|paths| paths.src.as_deref())
                                        .unwrap_or("src");
                                    let new_build = new_config
                                        .paths
                                        .as_ref()
                                        .and_then(|paths| paths.darklua_build.as_deref())
                                        .unwrap_or("darklua_build");

                                    if new_src != src || new_build != build {
                                        output::warn(
                                            "Source/build path changes require restarting `ezpm serve`; keeping the current watcher.",
                                        );
                                    } else {
                                        let new_settings = RojoProjectSettings::from_config(&new_config);
                                        if new_settings != rojo_settings {
                                            let new_targets = watch_targets(
                                                &project_dir,
                                                &src_path,
                                                &build_path,
                                                &new_settings,
                                            );
                                            match FileWatcher::with_targets(new_targets, &[]) {
                                                Ok((new_watcher, new_rx)) => {
                                                    match regenerate_rojo_project(
                                                        &project_dir,
                                                        &new_settings,
                                                    )
                                                    .await
                                                    {
                                                        Ok(written) => {
                                                            if written {
                                                                if let Err(error) = restart_rojo(
                                                                    &mut process_manager,
                                                                    &mut process_rx,
                                                                    port,
                                                                    &new_settings.generated_project,
                                                                )
                                                                .await
                                                                {
                                                                    output::error(&format!(
                                                                        "Rojo restart failed: {error}"
                                                                    ));
                                                                } else {
                                                                    rojo_restart_count = 0;
                                                                }
                                                            }
                                                            watcher = new_watcher;
                                                            watcher_rx = new_rx;
                                                            rojo_settings = new_settings;
                                                            output::info(
                                                                "Reloaded Rojo project settings; other ezpm.toml changes take effect after restart.",
                                                            );
                                                        }
                                                        Err(error) => output::error(&format!(
                                                            "Rojo config reload failed: {error:#}"
                                                        )),
                                                    }
                                                }
                                                Err(error) => output::error(&format!(
                                                    "Could not update file watcher: {error}"
                                                )),
                                            }
                                        } else {
                                            output::info(
                                                "ezpm.toml changed; restart serve to apply non-Rojo settings.",
                                            );
                                        }
                                    }
                                }
                                Err(error) => output::error(&format!(
                                    "Could not reload ezpm.toml: {error}"
                                )),
                            }
                        }

                        if project_changed {
                            match regenerate_rojo_project(&project_dir, &rojo_settings).await {
                                Ok(true) => {
                                    if let Err(error) = restart_rojo(
                                        &mut process_manager,
                                        &mut process_rx,
                                        port,
                                        &rojo_settings.generated_project,
                                    )
                                    .await
                                    {
                                        output::error(&format!("Rojo restart failed: {error}"));
                                    } else {
                                        rojo_restart_count = 0;
                                    }
                                }
                                Ok(false) => {}
                                Err(error) => output::error(&format!(
                                    "Rojo project regeneration failed: {error:#}"
                                )),
                            }
                        }

                        let source_changes = changes
                            .into_iter()
                            .filter(|change| {
                                !matches!(
                                    change,
                                    FileChange::RojoProjectChange(_) | FileChange::ConfigChange(_)
                                )
                            })
                            .collect::<Vec<_>>();
                        if !source_changes.is_empty() {
                            let context = ChangeContext {
                                src_prefix: &src,
                                source_root: &src_path,
                                build_root: &build_path,
                                aliases: &aliases,
                                require_fix_mode,
                                project_dir: &project_dir,
                                generated_project: &rojo_settings.generated_project,
                                file_changes_enabled,
                                fix_ctx: &fix_ctx,
                            };
                            handle_changes(
                                &source_changes,
                                &context,
                                &mut failed_files,
                            )
                            .await;
                        }
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
                        &mut rojo_restart_count,
                        port,
                        &rojo_settings.generated_project,
                    )
                    .await;
                }
            }
            _ = tokio::signal::ctrl_c() => {
                output::info("Stopping...");
                break;
            }
            _ = terminal_signal => {
                output::info("Terminal closed — stopping...");
                break;
            }
        }
    }

    process_manager.kill_all().await;
    drop(watcher);

    Ok(())
}

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
    path.strip_prefix(project_dir).unwrap_or(path).to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_require_fix_scope_matches_mode() {
        let changed = [FileChange::LuaChange(PathBuf::from("src/a.luau"))];
        let deleted = [FileChange::FileDeleted(PathBuf::from("src/a.luau"))];
        let metadata = [FileChange::MetaChange(PathBuf::from("src/init.meta.json"))];

        assert!(needs_full_require_fix(RequireFixMode::Strict, &changed));
        assert!(!needs_full_require_fix(RequireFixMode::Strict, &metadata));
        assert!(!needs_full_require_fix(RequireFixMode::Hybrid, &changed));
        assert!(needs_full_require_fix(RequireFixMode::Hybrid, &deleted));
        assert!(!needs_full_require_fix(RequireFixMode::Fast, &deleted));
    }

    #[test]
    fn sourcemap_refreshes_only_for_topology_changes() {
        assert!(!changes_project_topology(&[FileChange::LuaChange(
            PathBuf::from("src/a.luau")
        )]));
        assert!(!changes_project_topology(&[FileChange::MetaChange(
            PathBuf::from("src/init.meta.json")
        )]));
        assert!(changes_project_topology(&[FileChange::FileCreated(
            PathBuf::from("src/a.luau")
        )]));
        assert!(changes_project_topology(&[FileChange::DirectoryRemoved(
            PathBuf::from("src/components")
        )]));
    }
}

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::{
    config::{self, EzpmConfig, RequireFixMode},
    output,
    services::{
        config_gen,
        file_watcher::{FileChange, FileWatcher, WatchEvent, WatchTargets},
        process_manager::{ProcessEvent, ProcessManager},
        require_fixer::{self, is_lua_file},
        rojo_project::RojoProjectSettings,
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

async fn run_fix_requires(src: &str, context: &require_fixer::FixContext) -> bool {
    let src_clone = src.to_string();
    let context = context.clone();
    let result = tokio::task::spawn_blocking(move || {
        require_fixer::fix_requires_with_context(&PathBuf::from(&src_clone), &context)
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!("task panicked: {}", e)));

    match result {
        Ok(_) => true,
        Err(error) => {
            output::error(&format!("Require fix failed: {error}"));
            false
        }
    }
}

fn fix_context(
    project_dir: &Path,
    settings: &RojoProjectSettings,
    aliases: &HashMap<String, String>,
    src: &str,
) -> require_fixer::FixContext {
    require_fixer::FixContext::new_for_project(project_dir, &settings.project, aliases, src)
}

struct ChangeContext<'a> {
    src_prefix: &'a str,
    require_fix_mode: RequireFixMode,
    project_dir: &'a Path,
    source_project: &'a Path,
    file_changes_enabled: bool,
    fix_ctx: &'a require_fixer::FixContext,
    module_index: &'a require_fixer::ModuleIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ModuleFingerprint {
    bytes: u64,
    hash: u64,
}

#[derive(Debug, Default)]
struct ModuleSnapshot {
    by_fingerprint: HashMap<ModuleFingerprint, HashSet<PathBuf>>,
    by_path: HashMap<PathBuf, ModuleFingerprint>,
}

fn snapshot_modules(files: &[PathBuf]) -> ModuleSnapshot {
    let mut snapshot = ModuleSnapshot::default();
    for path in files {
        snapshot.refresh(path);
    }
    snapshot
}

impl ModuleSnapshot {
    fn refresh(&mut self, path: &Path) {
        self.remove(path);
        let Ok(contents) = std::fs::read(path) else {
            return;
        };
        let mut hasher = DefaultHasher::new();
        contents.hash(&mut hasher);
        let fingerprint = ModuleFingerprint {
            bytes: contents.len() as u64,
            hash: hasher.finish(),
        };
        let path = path.to_path_buf();
        self.by_fingerprint
            .entry(fingerprint)
            .or_default()
            .insert(path.clone());
        self.by_path.insert(path, fingerprint);
    }

    fn remove(&mut self, path: &Path) {
        let Some(fingerprint) = self.by_path.remove(path) else {
            return;
        };
        let Some(paths) = self.by_fingerprint.get_mut(&fingerprint) else {
            return;
        };
        paths.remove(path);
        if paths.is_empty() {
            self.by_fingerprint.remove(&fingerprint);
        }
    }
}

fn infer_module_moves(before: &ModuleSnapshot, after: &ModuleSnapshot) -> Vec<FileChange> {
    let mut moves = Vec::new();

    for (fingerprint, old_paths) in &before.by_fingerprint {
        let Some(new_paths) = after.by_fingerprint.get(fingerprint) else {
            continue;
        };
        if old_paths.len() != 1 || new_paths.len() != 1 {
            continue;
        }
        let old_path = old_paths.iter().next().expect("one old path");
        let new_path = new_paths.iter().next().expect("one new path");
        if old_path != new_path {
            moves.push(FileChange::FileRenamed {
                from: old_path.clone(),
                to: new_path.clone(),
            });
        }
    }

    moves
}

fn refresh_changed_modules(snapshot: &mut ModuleSnapshot, changes: &[FileChange]) {
    for change in changes {
        match change {
            FileChange::LuaChange(path) | FileChange::FileCreated(path) if is_lua_file(path) => {
                snapshot.refresh(path);
            }
            _ => {}
        }
    }
}

fn needs_full_require_fix(mode: RequireFixMode, changes: &[FileChange]) -> bool {
    match mode {
        RequireFixMode::Strict => changes
            .iter()
            .any(|change| !matches!(change, FileChange::MetaChange(_))),
        RequireFixMode::Hybrid => changes.iter().any(|change| {
            matches!(
                change,
                FileChange::FileDeleted(_)
                    | FileChange::FileRenamed { .. }
                    | FileChange::DirectoryRemoved(_)
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
                | FileChange::FileRenamed { .. }
                | FileChange::DirectoryCreated(_)
                | FileChange::DirectoryRemoved(_)
        )
    })
}

fn lone<'a>(
    changes: &'a [FileChange],
    select: impl Fn(&'a FileChange) -> Option<&'a Path>,
) -> Option<&'a Path> {
    let mut matches = changes.iter().filter_map(select);
    let path = matches.next()?;
    matches.next().is_none().then_some(path)
}

fn paired_rename(
    changes: &[FileChange],
    removed: impl Fn(&FileChange) -> Option<&Path>,
    created: impl Fn(&FileChange) -> Option<&Path>,
) -> Option<(PathBuf, PathBuf)> {
    let from = lone(changes, removed)?;
    let to = lone(changes, created)?;
    Some((from.to_path_buf(), to.to_path_buf()))
}

fn detected_renames(changes: &[FileChange]) -> Vec<(PathBuf, PathBuf)> {
    let explicit = changes
        .iter()
        .filter_map(|change| match change {
            FileChange::FileRenamed { from, to } => Some((from.clone(), to.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !explicit.is_empty() {
        return explicit;
    }

    let file_move = paired_rename(
        changes,
        |change| match change {
            FileChange::FileDeleted(path) if is_lua_file(path) => Some(path.as_path()),
            _ => None,
        },
        |change| match change {
            FileChange::FileCreated(path) if is_lua_file(path) => Some(path.as_path()),
            _ => None,
        },
    );
    let dir_move = || {
        paired_rename(
            changes,
            |change| match change {
                FileChange::DirectoryRemoved(path) => Some(path.as_path()),
                _ => None,
            },
            |change| match change {
                FileChange::DirectoryCreated(path) => Some(path.as_path()),
                _ => None,
            },
        )
    };

    file_move.or_else(dir_move).into_iter().collect()
}

async fn refresh_sourcemap(project_dir: &Path, project_file: &Path) -> bool {
    let project_dir = project_dir.to_path_buf();
    let project_file = project_file.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        sourcemap::generate_sourcemap_for_project(&project_dir, &project_file)
    })
    .await;

    match result {
        Ok(Ok(result)) if result.success => true,
        Ok(Ok(result)) => {
            output::error(&format!(
                "Sourcemap update failed: {}",
                result.stderr.trim()
            ));
            false
        }
        Ok(Err(error)) => {
            output::error(&format!("Sourcemap update failed: {error}"));
            false
        }
        Err(error) => {
            output::error(&format!("Sourcemap task panicked: {error}"));
            false
        }
    }
}

async fn handle_changes(
    changes: &[FileChange],
    context: &ChangeContext<'_>,
    failed_files: &mut HashSet<PathBuf>,
) {
    let t0 = Instant::now();
    let is_batch = changes.len() > 1;

    if changes_project_topology(changes) {
        refresh_sourcemap(context.project_dir, context.source_project).await;
    }

    let mut replacements = detected_renames(changes)
        .into_iter()
        .filter_map(|(from, to)| {
            let old_path = context
                .fix_ctx
                .game_require_for_source(context.project_dir, &from)?;
            let new_path = context
                .fix_ctx
                .game_require_for_source(context.project_dir, &to)?;
            (old_path != new_path).then_some((old_path, new_path))
        })
        .collect::<Vec<_>>();
    replacements.sort();
    replacements.dedup();

    if !replacements.is_empty() {
        let source_root = context.project_dir.join(context.src_prefix);
        if let Err(error) = require_fixer::rewrite_require_prefixes(&source_root, &replacements) {
            output::error(&format!("Could not update renamed requires: {error}"));
        }
    }

    let full_fix_performed = needs_full_require_fix(context.require_fix_mode, changes);
    if full_fix_performed && !run_fix_requires(context.src_prefix, context.fix_ctx).await {
        return;
    }

    for change in changes {
        match change {
            FileChange::LuaChange(path) => {
                handle_lua_file(path, context, failed_files, is_batch, full_fix_performed).await;
            }
            FileChange::FileCreated(path) => {
                if is_lua_file(path) {
                    handle_lua_file(path, context, failed_files, is_batch, full_fix_performed)
                        .await;
                }
            }
            FileChange::FileRenamed { to, .. } => {
                if is_lua_file(to) {
                    handle_lua_file(to, context, failed_files, is_batch, true).await;
                }
            }
            FileChange::MetaChange(_)
            | FileChange::FileDeleted(_)
            | FileChange::DirectoryCreated(_)
            | FileChange::DirectoryRemoved(_)
            | FileChange::RojoProjectChange(_)
            | FileChange::ConfigChange(_) => {}
        }
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

async fn handle_lua_file(
    path: &Path,
    context: &ChangeContext<'_>,
    failed_files: &mut HashSet<PathBuf>,
    is_batch: bool,
    full_fix_performed: bool,
) {
    let t0 = Instant::now();

    if !full_fix_performed && !matches!(context.require_fix_mode, RequireFixMode::Strict) {
        if let Err(e) =
            require_fixer::fix_single_file_with_index(path, context.fix_ctx, context.module_index)
        {
            output::error(&format!(
                "{}: require fix failed: {}",
                display_name(path),
                e
            ));
            failed_files.insert(path.to_path_buf());
            return;
        }
    }

    let filename = display_name(path);
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
                "Synced {} ({}ms)",
                filename,
                t0.elapsed().as_millis()
            ));
        }
    }
}

async fn handle_process_event(
    event: ProcessEvent,
    pm: &mut ProcessManager,
    restart_count: &mut u32,
    port: u16,
    project: &Path,
) {
    match event {
        ProcessEvent::Crashed { ref name, .. } if name == "rojo" => {
            if *restart_count < 1 {
                output::warn("Rojo exited unexpectedly — restarting...");
                let port_str = port.to_string();
                let project = project.to_string_lossy();
                if let Err(e) = pm
                    .spawn("rojo", "rojo", &["serve", &project, "--port", &port_str])
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
    settings: &RojoProjectSettings,
) -> WatchTargets {
    WatchTargets {
        source_root: source_root.to_path_buf(),
        project_files: vec![project_dir.join(&settings.project)],
        config_file: Some(project_dir.join("ezpm.toml")),
    }
}

async fn restart_rojo(
    process_manager: &mut ProcessManager,
    process_rx: &mut tokio::sync::mpsc::Receiver<ProcessEvent>,
    port: u16,
    project: &Path,
) -> anyhow::Result<()> {
    process_manager.kill("rojo").await;
    while process_rx.try_recv().is_ok() {}

    let port = port.to_string();
    let project = project.to_string_lossy();
    process_manager
        .spawn("rojo", "rojo", &["serve", &project, "--port", &port])
        .await
        .context("failed to restart Rojo")
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
    let mut aliases: HashMap<String, String> = config.aliases.clone().unwrap_or_default();
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

    config_gen::write_config_files(&project_dir, &aliases)
        .context("failed to generate .luaurc from ezpm.toml")?;

    sync_toolchain_versions(&project_dir)?;
    {
        let pb = output::start_spinner("Generating sourcemap...");
        let t0 = Instant::now();
        let project_dir_clone = project_dir.clone();
        let source_project = rojo_settings.project.clone();
        let result = tokio::task::spawn_blocking(move || {
            sourcemap::generate_sourcemap_for_project(&project_dir_clone, &source_project)
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

    let mut fix_ctx = fix_context(&project_dir, &rojo_settings, &aliases, &src);
    {
        let pb = output::start_spinner("Fixing require paths...");
        let t0 = Instant::now();
        let src_clone = src.clone();
        let context = fix_ctx.clone();
        let result = tokio::task::spawn_blocking(move || {
            require_fixer::fix_requires_with_context(&PathBuf::from(&src_clone), &context)
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

    let (mut watcher, mut watcher_rx) = {
        let pb = output::start_spinner("Starting file watcher...");
        let t0 = Instant::now();
        let result =
            FileWatcher::with_targets(watch_targets(&project_dir, &src_path, &rojo_settings), &[]);
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
        let project = rojo_settings.project.to_string_lossy();
        let (mut pm, rx) = ProcessManager::new();
        let result = pm
            .spawn("rojo", "rojo", &["serve", &project, "--port", &port_str])
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
    let source_files = require_fixer::lua_files(&src_path);
    let mut module_snapshot = snapshot_modules(&source_files);
    let mut module_index = require_fixer::ModuleIndex::from_files(&source_files);

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
                    Some(WatchEvent::Changes(mut changes)) => {
                        let topology_changed = changes_project_topology(&changes);
                        if topology_changed {
                            let files = require_fixer::lua_files(&src_path);
                            let current_snapshot = snapshot_modules(&files);
                            for inferred in infer_module_moves(&module_snapshot, &current_snapshot) {
                                if !changes.contains(&inferred) {
                                    changes.push(inferred);
                                }
                            }
                            module_index = require_fixer::ModuleIndex::from_files(&files);
                        }
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
                                    let new_aliases = new_config.aliases.clone().unwrap_or_default();
                                    match config_gen::write_config_files(&project_dir, &new_aliases) {
                                        Ok(()) => output::info("Regenerated .luaurc from ezpm.toml"),
                                        Err(error) => output::error(&format!(
                                            "Could not regenerate .luaurc: {error}"
                                        )),
                                    }
                                    let new_src = new_config
                                        .paths
                                        .as_ref()
                                        .and_then(|paths| paths.src.as_deref())
                                        .unwrap_or("src");
                                    if new_src != src {
                                        output::warn(
                                            "Source path changes require restarting `ezpm serve`; keeping the current watcher.",
                                        );
                                    } else {
                                        if new_aliases != aliases {
                                            aliases = new_aliases;
                                            fix_ctx = fix_context(
                                                &project_dir,
                                                &rojo_settings,
                                                &aliases,
                                                &src,
                                            );
                                            run_fix_requires(&src, &fix_ctx).await;
                                        }
                                        let new_settings = RojoProjectSettings::from_config(&new_config);
                                        if new_settings != rojo_settings {
                                            let new_targets = watch_targets(
                                                &project_dir,
                                                &src_path,
                                                &new_settings,
                                            );
                                            match FileWatcher::with_targets(new_targets, &[]) {
                                                Ok((new_watcher, new_rx)) => {
                                                    if let Err(error) = restart_rojo(
                                                        &mut process_manager,
                                                        &mut process_rx,
                                                        port,
                                                        &new_settings.project,
                                                    )
                                                    .await
                                                    {
                                                        output::error(&format!("Rojo restart failed: {error}"));
                                                    } else {
                                                        rojo_restart_count = 0;
                                                    }
                                                    watcher = new_watcher;
                                                    watcher_rx = new_rx;
                                                    rojo_settings = new_settings;
                                                    fix_ctx = fix_context(
                                                        &project_dir,
                                                        &rojo_settings,
                                                        &aliases,
                                                        &src,
                                                    );
                                                    output::info("Reloaded Rojo project settings; alias changes take effect after restart.");
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
                            if refresh_sourcemap(&project_dir, &rojo_settings.project).await {
                                fix_ctx =
                                    fix_context(&project_dir, &rojo_settings, &aliases, &src);
                                run_fix_requires(&src, &fix_ctx).await;
                            }
                            if let Err(error) = restart_rojo(
                                &mut process_manager,
                                &mut process_rx,
                                port,
                                &rojo_settings.project,
                            )
                            .await
                            {
                                output::error(&format!("Rojo restart failed: {error}"));
                            } else {
                                rojo_restart_count = 0;
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
                                require_fix_mode,
                                project_dir: &project_dir,
                                source_project: &rojo_settings.project,
                                file_changes_enabled,
                                fix_ctx: &fix_ctx,
                                module_index: &module_index,
                            };
                            handle_changes(
                                &source_changes,
                                &context,
                                &mut failed_files,
                            )
                            .await;
                            if topology_changed {
                                module_snapshot =
                                    snapshot_modules(&require_fixer::lua_files(&src_path));
                            } else {
                                refresh_changed_modules(&mut module_snapshot, &source_changes);
                            }
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
                        &rojo_settings.project,
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

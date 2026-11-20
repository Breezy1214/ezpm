use anyhow::{Context, Result};
use inquire::{Confirm, Select, Text};
use std::collections::{BTreeSet, HashMap};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::config;
use crate::error::EzpmError;
use crate::output;
pub use crate::services::rojo_project::generate_rojo_project;
use crate::services::toolchain;
use crate::services::{agents, atomic_file, config_gen, fs_txn::FileTransaction};

#[derive(Debug, Clone, Copy, Default)]
pub struct InitOptions {
    pub dry_run: bool,
    /// Add ezpm's rules for coding agents to AGENTS.md (on by default).
    pub agent_rules: bool,
    /// Write only the agent files, for a project that is already set up.
    pub agent_rules_only: bool,
    /// Register `ezpm hook stop` as a Claude Code Stop hook.
    pub agent_hook: bool,
}

pub fn run_init(options: InitOptions) -> Result<()> {
    if options.agent_rules_only {
        let mut txn = FileTransaction::new();
        let files = write_agent_files(&mut txn, true, options.agent_hook)?;
        txn.commit();
        report_files(&files);
        return Ok(());
    }

    let discovery = discover_rojo_projects(Path::new("."))?;

    for project in &discovery.projects {
        if let Some(error) = &project.parse_error {
            output::warn(&format!(
                "Could not parse {}: {error}",
                project.path.display()
            ));
        } else {
            output::info(&format!("Detected {}", project.path.display()));
        }
    }

    if options.dry_run {
        return print_dry_run(&discovery, options);
    }

    if output::is_json() || !std::io::stdin().is_terminal() {
        return Err(EzpmError::usage("ezpm init requires an interactive terminal")
            .with_hint(
                "use `ezpm init --dry-run` to preview, or `ezpm init --agent-rules` to add only the agent rules",
            )
            .into());
    }

    // Everything init writes goes through this, so a failure or Ctrl-C part-way through puts
    // the folder back as it was.
    let mut txn = FileTransaction::new();

    let has_ezpm = Path::new("ezpm.toml").exists();
    let has_rokit = Path::new("rokit.toml").exists();
    let has_wally = Path::new("wally.toml").exists();
    let selected_project = select_rojo_project(&discovery)?;
    let has_project_json = selected_project.is_some();

    if has_rokit {
        output::info("Detected rokit.toml");
    }
    if has_wally {
        output::info("Detected wally.toml");
    }

    if has_ezpm {
        let overwrite = Confirm::new("ezpm.toml already exists — overwrite?")
            .with_default(false)
            .prompt()?;
        if !overwrite {
            output::info("Aborted.");
            return Ok(());
        }
    }

    let detected_name = if let Some(project) = selected_project {
        project
            .name
            .clone()
            .unwrap_or_else(|| "my-roblox-game".to_string())
    } else {
        "my-roblox-game".to_string()
    };

    let project_name = Text::new("Project name:")
        .with_default(&detected_name)
        .prompt()?;

    if let Some(project) = selected_project {
        if let SourceRootInference::Ambiguous(candidates) = &project.source_root {
            output::warn(&format!(
                "Source root is ambiguous ({}); please choose it explicitly.",
                candidates.join(", ")
            ));
        }
    }
    let inferred_src = selected_project
        .and_then(|project| match &project.source_root {
            SourceRootInference::Inferred(path) => Some(path.as_str()),
            _ => None,
        })
        .unwrap_or("src");
    let src_dir = Text::new("Source directory:")
        .with_default(inferred_src)
        .prompt()?;
    let src_dir = match config::normalize_relative(&src_dir) {
        dir if dir.is_empty() => "src".to_string(),
        dir => dir,
    };

    if has_rokit {
        let choice =
            Select::new("rokit.toml already exists —", vec!["keep", "overwrite"]).prompt()?;
        if choice == "overwrite" {
            let rokit_content = generate_rokit_toml();
            txn.write(Path::new("rokit.toml"), rokit_content)
                .context("Failed to write rokit.toml")?;
            output::success("Generated rokit.toml");
        }
    } else {
        let create_rokit = Confirm::new("No rokit.toml found — create with default tools?")
            .with_default(true)
            .prompt()?;
        if create_rokit {
            let rokit_content = generate_rokit_toml();
            txn.write(Path::new("rokit.toml"), rokit_content)
                .context("Failed to write rokit.toml")?;
            output::success("Generated rokit.toml");
        }
    }

    let regenerate_project_json = !has_project_json;

    let mut aliases: HashMap<String, String> = HashMap::new();

    if aliases.is_empty() {
        let use_defaults =
            Confirm::new("Use default aliases (Client, Server, Shared, Packages, ServerPackages)?")
                .with_default(true)
                .prompt()?;

        if use_defaults {
            aliases.insert("Client".to_string(), format!("{}/client/", src_dir));
            aliases.insert("Server".to_string(), format!("{}/server/", src_dir));
            aliases.insert("Shared".to_string(), format!("{}/shared/", src_dir));
            aliases.insert("Packages".to_string(), "Packages/".to_string());
            aliases.insert("ServerPackages".to_string(), "ServerPackages/".to_string());
        }
    }

    let aliases: HashMap<String, String> = aliases
        .into_iter()
        .map(|(k, v)| {
            let path = if v.ends_with('/') {
                v
            } else {
                format!("{}/", v)
            };
            (k, path)
        })
        .collect();

    let alias_dirs = aliases.values().map(|path| path.trim_end_matches('/'));
    for dir_path in std::iter::once(src_dir.as_str()).chain(alias_dirs) {
        if txn
            .create_dir_all(Path::new(dir_path))
            .with_context(|| format!("Failed to create directory: {}", dir_path))?
        {
            output::success(&format!("Created directory: {}", dir_path));
        }
    }

    txn.track(Path::new("ezpm.toml"))?;
    config::save_ezpm_toml(Path::new("."), &project_name, &src_dir, &aliases)?;
    if let Some(project) = selected_project {
        record_rojo_template(Path::new("ezpm.toml"), &project.path)?;
    }
    output::success("Generated ezpm.toml");

    if !aliases.is_empty() {
        txn.track(Path::new(".luaurc"))?;
        config_gen::write_config_files(Path::new("."), &aliases)?;
        output::success("Generated .luaurc");
    }

    if regenerate_project_json {
        let project_json_str = generate_rojo_project(&project_name, &aliases, &src_dir);
        txn.write(Path::new("default.project.json"), project_json_str)
            .context("Failed to write default.project.json")?;
        output::success("Generated default.project.json");
    }

    if options.agent_rules || options.agent_hook {
        let files = write_agent_files(&mut txn, options.agent_rules, options.agent_hook)?;
        for (path, outcome) in files {
            if outcome == "wrote" {
                output::success(&format!("Updated {path}"));
            }
        }
    }

    txn.commit();
    output::success("Project initialized!");
    output::hint("Run `ezpm serve` to start developing.");

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRootInference {
    Inferred(String),
    Ambiguous(Vec<String>),
    None,
}

#[derive(Debug, Clone)]
pub struct RojoProjectCandidate {
    pub path: PathBuf,
    pub name: Option<String>,
    pub path_mappings: Vec<String>,
    pub source_root: SourceRootInference,
    pub parse_error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RojoDiscovery {
    pub projects: Vec<RojoProjectCandidate>,
}

impl RojoDiscovery {
    pub fn preferred(&self) -> Option<&RojoProjectCandidate> {
        self.projects.first()
    }
}

pub fn discover_rojo_projects(root: &Path) -> Result<RojoDiscovery> {
    let mut paths = std::fs::read_dir(root)
        .with_context(|| format!("Failed to inspect {}", root.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".project.json"))
        })
        .collect::<Vec<_>>();
    paths.sort_by(|a, b| {
        let a_default = a.file_name().and_then(|v| v.to_str()) == Some("default.project.json");
        let b_default = b.file_name().and_then(|v| v.to_str()) == Some("default.project.json");
        b_default.cmp(&a_default).then_with(|| a.cmp(b))
    });

    let projects = paths
        .into_iter()
        .map(|path| analyze_rojo_project(&path))
        .collect::<Vec<_>>();
    Ok(RojoDiscovery { projects })
}

fn analyze_rojo_project(path: &Path) -> RojoProjectCandidate {
    let parsed = std::fs::read_to_string(path)
        .map_err(anyhow::Error::from)
        .and_then(|contents| {
            serde_json::from_str::<serde_json::Value>(&contents).map_err(Into::into)
        });

    match parsed {
        Ok(json) => {
            let mut path_mappings = Vec::new();
            collect_path_mappings(&json, &mut path_mappings);
            path_mappings.sort();
            path_mappings.dedup();
            RojoProjectCandidate {
                path: path.to_path_buf(),
                name: json.get("name").and_then(|v| v.as_str()).map(str::to_owned),
                source_root: infer_source_root(&path_mappings),
                path_mappings,
                parse_error: None,
            }
        }
        Err(error) => RojoProjectCandidate {
            path: path.to_path_buf(),
            name: None,
            path_mappings: Vec::new(),
            source_root: SourceRootInference::None,
            parse_error: Some(error.to_string()),
        },
    }
}

fn collect_path_mappings(value: &serde_json::Value, paths: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(path) = object.get("$path").and_then(|value| value.as_str()) {
                paths.push(path.to_string());
            }
            for child in object.values() {
                collect_path_mappings(child, paths);
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                collect_path_mappings(child, paths);
            }
        }
        _ => {}
    }
}

pub fn infer_source_root(paths: &[String]) -> SourceRootInference {
    let mut candidates = BTreeSet::new();
    for path in paths {
        let normalized = path.replace('\\', "/");
        let components = normalized
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect::<Vec<_>>();
        if components.first().is_some_and(|part| *part == "..") {
            continue;
        }
        if components.len() == 1 && components[0].eq_ignore_ascii_case("src") {
            candidates.insert(components[0].to_string());
            continue;
        }
        if let Some(layer_index) = components.iter().position(|component| {
            matches!(
                component.to_ascii_lowercase().as_str(),
                "client" | "server" | "shared"
            )
        }) {
            if layer_index > 0 {
                candidates.insert(components[..layer_index].join("/"));
            }
        }
    }
    match candidates.len() {
        0 => SourceRootInference::None,
        1 => SourceRootInference::Inferred(candidates.into_iter().next().unwrap()),
        _ => SourceRootInference::Ambiguous(candidates.into_iter().collect()),
    }
}

fn select_rojo_project(discovery: &RojoDiscovery) -> Result<Option<&RojoProjectCandidate>> {
    if discovery.projects.is_empty() {
        return Ok(None);
    }
    let selected = if discovery.projects.len() == 1
        || discovery.projects[0]
            .path
            .file_name()
            .and_then(|v| v.to_str())
            == Some("default.project.json")
    {
        &discovery.projects[0]
    } else {
        let labels = discovery
            .projects
            .iter()
            .map(|project| project.path.display().to_string())
            .collect::<Vec<_>>();
        let selected = Select::new("Select the Rojo project to adopt:", labels).prompt()?;
        discovery
            .projects
            .iter()
            .find(|project| project.path.display().to_string() == selected)
            .unwrap()
    };
    if let Some(error) = &selected.parse_error {
        anyhow::bail!(
            "Cannot adopt malformed Rojo project {}: {error}",
            selected.path.display()
        );
    }
    Ok(Some(selected))
}

fn print_dry_run(discovery: &RojoDiscovery, options: InitOptions) -> Result<()> {
    let selected = if discovery.projects.is_empty() {
        None
    } else if discovery.projects.len() == 1
        || discovery.projects[0]
            .path
            .file_name()
            .and_then(|v| v.to_str())
            == Some("default.project.json")
    {
        Some(&discovery.projects[0])
    } else {
        anyhow::bail!("Multiple Rojo projects found; run `ezpm init` interactively to select one");
    };
    let mut files: Vec<(String, &'static str)> = Vec::new();
    if let Some(project) = selected {
        if let Some(error) = &project.parse_error {
            anyhow::bail!(
                "Cannot adopt malformed Rojo project {}: {error}",
                project.path.display()
            );
        }
        if let SourceRootInference::Ambiguous(candidates) = &project.source_root {
            anyhow::bail!(
                "Source root is ambiguous ({}); run `ezpm init` interactively",
                candidates.join(", ")
            );
        }
        output::info(&format!(
            "Would preserve Rojo template: {}",
            project.path.display()
        ));
        let path = project.path.strip_prefix(".").unwrap_or(&project.path);
        files.push((path.display().to_string(), "unchanged"));
    } else {
        output::info("Would create: default.project.json");
        files.push(("default.project.json".into(), "wouldWrite"));
    }
    for path in ["ezpm.toml", ".luaurc"] {
        output::info(&format!("Would create or update: {path}"));
        files.push((path.into(), "wouldWrite"));
    }
    if !Path::new("rokit.toml").exists() {
        output::info("Would create: rokit.toml");
        files.push(("rokit.toml".into(), "wouldWrite"));
    }
    if options.agent_rules {
        let current = std::fs::read_to_string(agents::AGENTS_FILE).ok();
        if agents::upsert_rules(current.as_deref()).is_some() {
            output::info("Would add ezpm's agent rules to: AGENTS.md");
            files.push((agents::AGENTS_FILE.into(), "wouldWrite"));
        }
    }

    if output::is_json() {
        output::document(&serde_json::json!({
            "dry_run": true,
            "changed": false,
            "files": files_json(&files),
        }));
    } else {
        output::success("Dry run complete; no files were changed.");
    }
    Ok(())
}

/// Adds the AGENTS.md rules (and the CLAUDE.md import) and, when asked, the Claude Code Stop
/// hook. Returns each file with `wrote` or `unchanged`.
fn write_agent_files(
    txn: &mut FileTransaction,
    rules: bool,
    hook: bool,
) -> Result<Vec<(String, &'static str)>> {
    let mut changes: Vec<(&str, Option<String>)> = Vec::new();
    let read = |path: &str| std::fs::read_to_string(path).ok();
    if rules {
        let agents_md = read(agents::AGENTS_FILE);
        changes.push((
            agents::AGENTS_FILE,
            agents::upsert_rules(agents_md.as_deref()),
        ));
        let claude_md = read(agents::CLAUDE_FILE);
        changes.push((
            agents::CLAUDE_FILE,
            agents::with_claude_import(claude_md.as_deref()),
        ));
    }
    if hook {
        let settings = read(agents::CLAUDE_SETTINGS_FILE);
        changes.push((
            agents::CLAUDE_SETTINGS_FILE,
            agents::with_claude_stop_hook(settings.as_deref())?,
        ));
    }

    let mut files = Vec::new();
    for (path, contents) in changes {
        let outcome = match contents {
            Some(contents) => {
                txn.write(Path::new(path), contents)
                    .with_context(|| format!("Failed to write {path}"))?;
                "wrote"
            }
            None => "unchanged",
        };
        files.push((path.to_string(), outcome));
    }
    Ok(files)
}

fn files_json(files: &[(String, &str)]) -> Vec<serde_json::Value> {
    files
        .iter()
        .map(|(path, outcome)| serde_json::json!({ "path": path, "outcome": outcome }))
        .collect()
}

fn report_files(files: &[(String, &'static str)]) {
    if output::is_json() {
        output::document(&serde_json::json!({
            "changed": files.iter().any(|(_, outcome)| *outcome == "wrote"),
            "files": files_json(files),
        }));
        return;
    }
    for (path, outcome) in files {
        if *outcome == "wrote" {
            output::success(&format!("Updated {path}"));
        } else {
            output::info(&format!("{path} is already up to date"));
        }
    }
}

fn record_rojo_template(config_path: &Path, project_path: &Path) -> Result<()> {
    let contents = std::fs::read_to_string(config_path)?;
    let mut value: toml::Value = toml::from_str(&contents)?;
    let table = value
        .as_table_mut()
        .context("ezpm.toml root must be a table")?;
    let mut rojo = toml::map::Map::new();
    let project_path = project_path.strip_prefix(".").unwrap_or(project_path);
    rojo.insert(
        "project".into(),
        toml::Value::String(project_path.display().to_string()),
    );
    table.insert("rojo".into(), toml::Value::Table(rojo));
    atomic_file::write(config_path, toml::to_string_pretty(&value)?)?;
    Ok(())
}

pub fn generate_rokit_toml() -> String {
    toolchain::render_default_rokit_toml(Some("# Generated by ezpm init"))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use tempfile::TempDir;

    fn write_project(dir: &Path, name: &str, contents: &str) {
        fs::write(dir.join(name), contents).expect("write project fixture");
    }

    #[test]
    fn discovery_prefers_default_then_sorts_lexically() {
        let dir = TempDir::new().unwrap();
        for name in ["z.project.json", "default.project.json", "a.project.json"] {
            write_project(dir.path(), name, r#"{"name":"game","tree":{}}"#);
        }

        let discovery = discover_rojo_projects(dir.path()).unwrap();
        let names = discovery
            .projects
            .iter()
            .map(|project| project.path.file_name().unwrap().to_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            ["default.project.json", "a.project.json", "z.project.json"]
        );
        assert_eq!(discovery.preferred().unwrap().name.as_deref(), Some("game"));
    }

    #[test]
    fn malformed_project_is_reported_without_hiding_other_candidates() {
        let dir = TempDir::new().unwrap();
        write_project(dir.path(), "bad.project.json", "{not-json");
        write_project(
            dir.path(),
            "good.project.json",
            r#"{"name":"valid","tree":{}}"#,
        );

        let discovery = discover_rojo_projects(dir.path()).unwrap();
        assert_eq!(discovery.projects.len(), 2);
        let bad = discovery
            .projects
            .iter()
            .find(|project| project.path.ends_with("bad.project.json"))
            .unwrap();
        assert!(bad.parse_error.is_some());
    }

    #[test]
    fn nested_path_mappings_are_collected_and_infer_source_root() {
        let dir = TempDir::new().unwrap();
        write_project(
            dir.path(),
            "default.project.json",
            r#"{
                "name":"nested",
                "tree":{"ReplicatedStorage":{"Shared":{"$path":"game/src/shared"}},
                "ServerScriptService":{"Server":{"children":[{"$path":"game/src/server"}]}}}
            }"#,
        );

        let discovery = discover_rojo_projects(dir.path()).unwrap();
        let project = discovery.preferred().unwrap();
        assert_eq!(
            project.path_mappings,
            ["game/src/server", "game/src/shared"]
        );
        assert_eq!(
            project.source_root,
            SourceRootInference::Inferred("game/src".into())
        );
    }

    #[test]
    fn conflicting_layer_prefixes_are_ambiguous() {
        let paths = vec!["game/src/client".into(), "legacy/server".into()];
        assert_eq!(
            infer_source_root(&paths),
            SourceRootInference::Ambiguous(vec!["game/src".into(), "legacy".into()])
        );
    }
}

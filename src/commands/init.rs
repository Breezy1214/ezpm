use anyhow::{Context, Result};
use inquire::{Confirm, MultiSelect, Select, Text};
use std::collections::{BTreeSet, HashMap};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::config;
use crate::output;
use crate::services::config_gen;
pub use crate::services::rojo_project::generate_rojo_project;
use crate::services::toolchain;

pub fn run_init(dry_run: bool) -> Result<()> {
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

    if dry_run {
        return print_dry_run(&discovery);
    }

    if !std::io::stdin().is_terminal() {
        anyhow::bail!("ezpm init requires an interactive terminal");
    }

    let has_ezpm = Path::new("ezpm.toml").exists();
    let has_darklua = Path::new(".darklua.json").exists();
    let has_rokit = Path::new("rokit.toml").exists();
    let has_wally = Path::new("wally.toml").exists();
    let selected_project = select_rojo_project(&discovery)?;
    let has_project_json = selected_project.is_some();

    if has_darklua {
        output::info("Detected .darklua.json");
    }
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

    if has_rokit {
        let choice =
            Select::new("rokit.toml already exists —", vec!["keep", "overwrite"]).prompt()?;
        if choice == "overwrite" {
            let rokit_content = generate_rokit_toml();
            std::fs::write("rokit.toml", rokit_content).context("Failed to write rokit.toml")?;
            output::success("Generated rokit.toml");
        }
    } else {
        let create_rokit = Confirm::new("No rokit.toml found — create with default tools?")
            .with_default(true)
            .prompt()?;
        if create_rokit {
            let rokit_content = generate_rokit_toml();
            std::fs::write("rokit.toml", rokit_content).context("Failed to write rokit.toml")?;
            output::success("Generated rokit.toml");
        }
    }

    let regenerate_project_json = !has_project_json;

    let mut aliases: HashMap<String, String> = HashMap::new();

    if has_darklua {
        let imported = config::import_aliases_from_dir(Path::new("."));
        if !imported.is_empty() {
            let mut alias_names: Vec<String> = imported.keys().cloned().collect();
            alias_names.sort();

            let selected = MultiSelect::new("Select aliases to import:", alias_names.clone())
                .with_all_selected_by_default()
                .prompt()?;

            for name in selected {
                if let Some(path) = imported.get(&name) {
                    aliases.insert(name, path.clone());
                }
            }
        }
    }

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

    if !Path::new(&src_dir).exists() {
        std::fs::create_dir_all(&src_dir)
            .with_context(|| format!("Failed to create directory: {}", src_dir))?;
        output::success(&format!("Created directory: {}", src_dir));
    }

    for alias_path in aliases.values() {
        let dir_path = alias_path.trim_end_matches('/');
        let path = Path::new(dir_path);
        if !path.exists() {
            std::fs::create_dir_all(path)
                .with_context(|| format!("Failed to create directory: {}", dir_path))?;
            output::success(&format!("Created directory: {}", dir_path));
        }
    }

    config::save_ezpm_toml(
        Path::new("."),
        &project_name,
        &src_dir,
        "darklua_build",
        &aliases,
    )?;
    if let Some(project) = selected_project {
        record_rojo_template(Path::new("ezpm.toml"), &project.path)?;
    }
    output::success("Generated ezpm.toml");

    if !aliases.is_empty() {
        config_gen::write_config_files(Path::new("."), &aliases, None)?;
        output::success("Generated .darklua.json");
        output::success("Generated .luaurc");
    }

    if regenerate_project_json {
        let project_json_str = generate_rojo_project(&project_name, &aliases, &src_dir);
        std::fs::write("default.project.json", project_json_str)
            .context("Failed to write default.project.json")?;
        output::success("Generated default.project.json");
    }

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

fn print_dry_run(discovery: &RojoDiscovery) -> Result<()> {
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
        output::info("Would generate: build.project.json");
    } else {
        output::info("Would create: default.project.json");
    }
    for path in ["ezpm.toml", ".darklua.json", ".luaurc"] {
        output::info(&format!("Would create or update: {path}"));
    }
    if !Path::new("rokit.toml").exists() {
        output::info("Would create: rokit.toml");
    }
    output::success("Dry run complete; no files were changed.");
    Ok(())
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
    rojo.insert(
        "generated_project".into(),
        toml::Value::String("build.project.json".into()),
    );
    table.insert("rojo".into(), toml::Value::Table(rojo));
    std::fs::write(config_path, toml::to_string_pretty(&value)?)?;
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

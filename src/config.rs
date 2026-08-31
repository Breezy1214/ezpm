use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct CheckConfig {
    pub entry_points: Option<Vec<String>>,
    pub layers: Option<HashMap<String, String>>,
    pub forbid: Option<Vec<ForbidRule>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ForbidRule {
    pub from: String,
    pub to: String,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct EzpmConfig {
    pub project: Option<ProjectConfig>,
    pub paths: Option<PathsConfig>,
    pub display: Option<DisplayConfig>,
    pub aliases: Option<HashMap<String, String>>,
    pub serve: Option<ServeConfig>,
    pub rojo: Option<RojoConfig>,
    pub check: Option<CheckConfig>,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct RojoConfig {
    pub project: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct ProjectConfig {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct PathsConfig {
    pub src: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct DisplayConfig {
    pub file_changes: Option<bool>,
    pub docs_enabled: Option<bool>,
    pub logs_enabled: Option<bool>,
    pub check_updates: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct ServeConfig {
    pub port: Option<u16>,
    pub require_fix_mode: Option<RequireFixMode>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RequireFixMode {
    Strict,
    #[default]
    Hybrid,
    Fast,
}

#[derive(Serialize)]
struct EzpmTomlOutput {
    project: ProjectTomlOutput,
    paths: PathsTomlOutput,
    display: DisplayTomlOutput,
    aliases: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct ProjectTomlOutput {
    name: String,
}

#[derive(Serialize)]
struct PathsTomlOutput {
    src: String,
}

#[derive(Serialize)]
struct DisplayTomlOutput {
    file_changes: bool,
    docs_enabled: bool,
    logs_enabled: bool,
    check_updates: bool,
}

pub fn save_ezpm_toml(
    dir: &Path,
    project_name: &str,
    src_dir: &str,
    aliases: &HashMap<String, String>,
) -> Result<()> {
    let output = EzpmTomlOutput {
        project: ProjectTomlOutput {
            name: project_name.to_string(),
        },
        paths: PathsTomlOutput {
            src: src_dir.to_string(),
        },
        display: DisplayTomlOutput {
            file_changes: true,
            docs_enabled: false,
            logs_enabled: true,
            check_updates: true,
        },
        aliases: aliases
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    };

    let toml_str = toml::to_string_pretty(&output)
        .map_err(|e| anyhow::anyhow!("Failed to serialize ezpm.toml: {}", e))?;
    std::fs::write(dir.join("ezpm.toml"), toml_str)?;
    Ok(())
}

pub fn save_aliases_preserving_config(
    dir: &Path,
    project_name: &str,
    src_dir: &str,
    aliases: &HashMap<String, String>,
) -> Result<()> {
    let toml_path = dir.join("ezpm.toml");

    if !toml_path.exists() {
        return save_ezpm_toml(dir, project_name, src_dir, aliases);
    }

    let contents = std::fs::read_to_string(&toml_path)
        .map_err(|e| anyhow::anyhow!("Failed to read ezpm.toml: {}", e))?;

    let mut root: toml::Value = toml::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("Failed to parse ezpm.toml: {}", e))?;

    if !root.is_table() {
        root = toml::Value::Table(toml::map::Map::new());
    }

    let root_table = root
        .as_table_mut()
        .expect("root must be a TOML table after normalization");

    if !root_table.contains_key("project") {
        let mut project_table = toml::map::Map::new();
        project_table.insert(
            "name".to_string(),
            toml::Value::String(project_name.to_string()),
        );
        root_table.insert("project".to_string(), toml::Value::Table(project_table));
    }

    if !root_table.contains_key("paths") {
        let mut paths_table = toml::map::Map::new();
        paths_table.insert("src".to_string(), toml::Value::String(src_dir.to_string()));
        root_table.insert("paths".to_string(), toml::Value::Table(paths_table));
    }

    let mut aliases_table = toml::map::Map::new();
    let mut sorted_aliases: Vec<(&String, &String)> = aliases.iter().collect();
    sorted_aliases.sort_by(|(a, _), (b, _)| a.cmp(b));

    for (name, path) in sorted_aliases {
        aliases_table.insert(name.clone(), toml::Value::String(path.clone()));
    }

    root_table.insert("aliases".to_string(), toml::Value::Table(aliases_table));

    let toml_str = toml::to_string_pretty(&root)
        .map_err(|e| anyhow::anyhow!("Failed to serialize ezpm.toml: {}", e))?;
    std::fs::write(toml_path, toml_str)?;

    Ok(())
}

pub fn load_config_from_str(input: &str) -> Result<(EzpmConfig, Vec<String>)> {
    if input.trim().is_empty() {
        return Ok((EzpmConfig::default(), vec![]));
    }

    let mut unknown_fields: BTreeSet<String> = BTreeSet::new();

    let de = toml::Deserializer::new(input);
    let config: EzpmConfig = serde_ignored::deserialize(de, |path| {
        unknown_fields.insert(path.to_string());
    })
    .map_err(|e| anyhow::anyhow!("Failed to parse ezpm.toml: {}", e))?;

    let warnings: Vec<String> = unknown_fields
        .into_iter()
        .map(|field| format!("Warning: unknown field '{}' in ezpm.toml", field))
        .collect();

    Ok((config, warnings))
}

pub fn load_config() -> Result<(EzpmConfig, Vec<String>)> {
    let toml_path = Path::new("ezpm.toml");
    if toml_path.exists() {
        let contents = std::fs::read_to_string(toml_path)
            .map_err(|e| anyhow::anyhow!("Failed to read ezpm.toml: {}", e))?;
        load_config_from_str(&contents)
    } else {
        Ok((EzpmConfig::default(), vec![]))
    }
}

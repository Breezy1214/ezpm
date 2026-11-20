use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use crate::diagnostics::{self, Diagnostic};
use crate::services::atomic_file;

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct CheckConfig {
    pub entry_points: Option<Vec<String>>,
    pub layers: Option<HashMap<String, String>>,
    pub forbid: Option<Vec<ForbidRule>>,
    pub feature_roots: Option<Vec<String>>,
    pub shared_features: Option<Vec<String>>,
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

impl EzpmConfig {
    pub fn src_dir(&self) -> String {
        let src = self
            .paths
            .as_ref()
            .and_then(|paths| paths.src.as_deref())
            .map(normalize_relative)
            .unwrap_or_default();
        if src.is_empty() {
            "src".to_string()
        } else {
            src
        }
    }
}

pub fn normalize_relative(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    let path = path.strip_prefix("./").unwrap_or(&path);
    path.trim_end_matches('/').to_string()
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
    pub logs_enabled: Option<bool>,
    pub check_updates: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct ServeConfig {
    pub port: Option<u16>,
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
    atomic_file::write(&dir.join("ezpm.toml"), toml_str)?;
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
    sorted_aliases.sort_by_key(|(name, _)| *name);

    for (name, path) in sorted_aliases {
        aliases_table.insert(name.clone(), toml::Value::String(path.clone()));
    }

    root_table.insert("aliases".to_string(), toml::Value::Table(aliases_table));

    let toml_str = toml::to_string_pretty(&root)
        .map_err(|e| anyhow::anyhow!("Failed to serialize ezpm.toml: {}", e))?;
    atomic_file::write(&toml_path, toml_str)?;

    Ok(())
}

pub fn load_config_from_str(input: &str) -> Result<(EzpmConfig, Vec<Diagnostic>)> {
    if input.trim().is_empty() {
        return Ok((EzpmConfig::default(), vec![]));
    }

    let mut unknown_fields: BTreeSet<String> = BTreeSet::new();

    let de = toml::Deserializer::parse(input)
        .map_err(|e| anyhow::anyhow!("Failed to parse ezpm.toml: {}", e))?;
    let config: EzpmConfig = serde_ignored::deserialize(de, |path| {
        unknown_fields.insert(path.to_string());
    })
    .map_err(|e| anyhow::anyhow!("Failed to parse ezpm.toml: {}", e))?;

    let warnings = unknown_fields
        .into_iter()
        .map(|field| unknown_field_diagnostic(&field))
        .collect();

    Ok((config, warnings))
}

fn known_fields(section: &str) -> &'static [&'static str] {
    match section {
        "" => &[
            "project", "paths", "display", "aliases", "serve", "rojo", "check",
        ],
        "project" => &["name"],
        "paths" => &["src"],
        "display" => &["file_changes", "logs_enabled", "check_updates"],
        "serve" => &["port"],
        "rojo" => &["project"],
        "check" => &[
            "entry_points",
            "layers",
            "forbid",
            "feature_roots",
            "shared_features",
        ],
        "check.forbid" => &["from", "to", "reason"],
        _ => &[],
    }
}

fn unknown_field_diagnostic(raw_path: &str) -> Diagnostic {
    // serde_ignored marks Option layers with `?` segments: `check.?.forbid.?.0.reson`.
    let segments: Vec<&str> = raw_path.split('.').filter(|s| *s != "?").collect();
    let path = segments.join(".");
    let (key, parents) = segments.split_last().expect("split yields one segment");
    let section = parents
        .iter()
        .filter(|segment| segment.parse::<usize>().is_err())
        .copied()
        .collect::<Vec<_>>()
        .join(".");
    let mut message = format!("unknown field '{path}' in ezpm.toml");
    if let Some(suggestion) = diagnostics::closest(key, known_fields(&section)) {
        message.push_str(&format!(". Did you mean '{suggestion}'?"));
    }
    Diagnostic::new("config.unknownField", message).at("ezpm.toml", None)
}

pub fn load_config() -> Result<(EzpmConfig, Vec<Diagnostic>)> {
    let toml_path = Path::new("ezpm.toml");
    if toml_path.exists() {
        let contents = std::fs::read_to_string(toml_path)
            .map_err(|e| anyhow::anyhow!("Failed to read ezpm.toml: {}", e))?;
        load_config_from_str(&contents)
    } else {
        Ok((EzpmConfig::default(), vec![]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_field_suggests_the_closest_key_in_its_section() {
        let (_, warnings) =
            load_config_from_str("[check]\nentry_point = []\n[[check.forbid]]\nfrom = \"a\"\nto = \"b\"\nreson = \"x\"\n")
                .unwrap();
        let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert!(
            messages
                .iter()
                .any(|m| m.contains("Did you mean 'entry_points'?")),
            "{messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|m| m.contains("Did you mean 'reason'?")),
            "{messages:?}"
        );
        assert!(warnings.iter().all(|w| w.code == "config.unknownField"));
    }

    #[test]
    fn normalize_relative_strips_dot_prefix_and_trailing_slashes() {
        assert_eq!(normalize_relative(" ./src/shared/ "), "src/shared");
        assert_eq!(normalize_relative("game\\client\\"), "game/client");
    }

    #[test]
    fn src_dir_defaults_when_missing_or_empty() {
        let mut config = EzpmConfig::default();
        assert_eq!(config.src_dir(), "src");
        config.paths = Some(PathsConfig {
            src: Some("/".to_string()),
        });
        assert_eq!(config.src_dir(), "src");
    }
}

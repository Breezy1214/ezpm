use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

// ─── Check (dependency analysis) config ──────────────────────────────────────

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
    pub check: Option<CheckConfig>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct ProjectConfig {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct PathsConfig {
    pub src: Option<String>,
    pub darklua_build: Option<String>,
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
    /// Default port is 34872
    pub port: Option<u16>,
    /// strict | hybrid | fast (default: hybrid)
    pub require_fix_mode: Option<RequireFixMode>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RequireFixMode {
    Strict,
    Hybrid,
    Fast,
}

impl Default for RequireFixMode {
    fn default() -> Self {
        Self::Hybrid
    }
}

/// Serialization-only struct for writing ezpm.toml.
/// Uses BTreeMap for deterministic alias ordering.
/// Field order matters: aliases (a table section) MUST be last.
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
    darklua_build: String,
}

#[derive(Serialize)]
struct DisplayTomlOutput {
    file_changes: bool,
    docs_enabled: bool,
    logs_enabled: bool,
    check_updates: bool,
}

/// Write an ezpm.toml file to `dir` with the given configuration values.
///
/// Uses `toml::to_string_pretty` with a dedicated serialization struct
/// to ensure deterministic output (BTreeMap for aliases, Pitfall 3 ordering).
pub fn save_ezpm_toml(
    dir: &Path,
    project_name: &str,
    src_dir: &str,
    darklua_build: &str,
    aliases: &HashMap<String, String>,
) -> Result<()> {
    let output = EzpmTomlOutput {
        project: ProjectTomlOutput {
            name: project_name.to_string(),
        },
        paths: PathsTomlOutput {
            src: src_dir.to_string(),
            darklua_build: darklua_build.to_string(),
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

/// Update only the `[aliases]` table in `ezpm.toml`, preserving all other
pub fn save_aliases_preserving_config(
    dir: &Path,
    project_name: &str,
    src_dir: &str,
    darklua_build: &str,
    aliases: &HashMap<String, String>,
) -> Result<()> {
    let toml_path = dir.join("ezpm.toml");

    if !toml_path.exists() {
        return save_ezpm_toml(dir, project_name, src_dir, darklua_build, aliases);
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
        paths_table.insert(
            "darklua_build".to_string(),
            toml::Value::String(darklua_build.to_string()),
        );
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

/// Parse an ezpm.toml string, returning the config and any warnings about unknown fields.
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

/// Try to import aliases from .luaurc or .darklua.json in the current directory.
/// .luaurc is preferred if it exists; falls back to .darklua.json.
pub fn import_aliases_from_darklua() -> HashMap<String, String> {
    import_aliases_from_dir(Path::new("."))
}

/// Import aliases from a specific directory.
pub fn import_aliases_from_dir(dir: &Path) -> HashMap<String, String> {
    // Try .luaurc first
    let luaurc_path = dir.join(".luaurc");
    if luaurc_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&luaurc_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) {
                if let Some(aliases_obj) = json.get("aliases").and_then(|v| v.as_object()) {
                    let mut aliases = HashMap::new();
                    for (key, value) in aliases_obj {
                        // Skip the lune typedef path
                        if key == "lune" {
                            continue;
                        }
                        if let Some(path) = value.as_str() {
                            aliases.insert(key.clone(), path.to_string());
                        }
                    }
                    if !aliases.is_empty() {
                        return aliases;
                    }
                }
            }
        }
    }

    // Fall back to .darklua.json
    let darklua_path = dir.join(".darklua.json");
    if darklua_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&darklua_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&contents) {
                // Navigate to process[0].current.aliases
                let current = json
                    .get("process")
                    .and_then(|p| p.get(0))
                    .and_then(|p| p.get("current"));
                let alias_obj = current
                    .and_then(|c| c.get("aliases"))
                    .or_else(|| current.and_then(|c| c.get("sources")))
                    .and_then(|s| s.as_object());
                if let Some(obj) = alias_obj {
                    let mut aliases = HashMap::new();
                    for (key, value) in obj {
                        // Strip leading @ from alias name
                        let name = key.strip_prefix('@').unwrap_or(key);
                        if let Some(path) = value.as_str() {
                            aliases.insert(name.to_string(), path.to_string());
                        }
                    }
                    return aliases;
                }
            }
        }
    }

    HashMap::new()
}

/// Load config from the ezpm.toml in the current directory.
pub fn load_config() -> Result<(EzpmConfig, Vec<String>)> {
    let toml_path = Path::new("ezpm.toml");
    let (mut config, warnings) = if toml_path.exists() {
        let contents = std::fs::read_to_string(toml_path)
            .map_err(|e| anyhow::anyhow!("Failed to read ezpm.toml: {}", e))?;
        load_config_from_str(&contents)?
    } else {
        (EzpmConfig::default(), vec![])
    };

    // Auto-import aliases from .luaurc or .darklua.json if no aliases in config
    let has_aliases = config
        .aliases
        .as_ref()
        .map(|m| !m.is_empty())
        .unwrap_or(false);

    if !has_aliases {
        let imported = import_aliases_from_darklua();
        if !imported.is_empty() {
            config.aliases = Some(imported);
        }
    }

    Ok((config, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_ezpm_toml_roundtrip() {
        let dir = TempDir::new().expect("temp dir");
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("Server".to_string(), "src/server/".to_string());

        save_ezpm_toml(dir.path(), "test-project", "src", "darklua_build", &aliases)
            .expect("save must succeed");

        let contents =
            std::fs::read_to_string(dir.path().join("ezpm.toml")).expect("must read ezpm.toml");

        // Verify it can be parsed back
        let (config, warnings) = load_config_from_str(&contents).expect("must parse");
        assert!(warnings.is_empty(), "no unknown fields expected");
        assert_eq!(
            config.project.as_ref().and_then(|p| p.name.as_deref()),
            Some("test-project")
        );
        let loaded_aliases = config.aliases.unwrap_or_default();
        assert_eq!(
            loaded_aliases.get("Client"),
            Some(&"src/client/".to_string())
        );
        assert_eq!(
            loaded_aliases.get("Server"),
            Some(&"src/server/".to_string())
        );
    }

    #[test]
    fn test_load_config_parses_require_fix_mode() {
        let input = r#"
[serve]
port = 34872
require_fix_mode = "fast"
"#;

        let (config, warnings) = load_config_from_str(input).expect("must parse");
        assert!(warnings.is_empty(), "no warnings expected");
        assert_eq!(
            config.serve.as_ref().and_then(|s| s.require_fix_mode),
            Some(RequireFixMode::Fast)
        );
    }
}

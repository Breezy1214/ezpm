use anyhow::Result;
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

#[derive(Debug, Deserialize, Default)]
pub struct EzpmConfig {
    pub project: Option<ProjectConfig>,
    pub paths: Option<PathsConfig>,
    pub display: Option<DisplayConfig>,
    pub aliases: Option<HashMap<String, String>>,
    pub serve: Option<ServeConfig>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ProjectConfig {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct PathsConfig {
    pub src: Option<String>,
    pub darklua_build: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct DisplayConfig {
    pub file_changes: Option<bool>,
    pub docs_enabled: Option<bool>,
    pub logs_enabled: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ServeConfig {
    /// Default port is 34872
    pub port: Option<u16>,
}

/// Parse an ezpm.toml string, returning the config and any warnings about unknown fields.
/// Unknown fields produce warnings but do not cause errors.
/// An empty string returns defaults with no warnings.
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
/// The `lune` alias is skipped (it's a typedef path, not a user alias).
pub fn import_aliases_from_darklua() -> HashMap<String, String> {
    import_aliases_from_dir(Path::new("."))
}

/// Import aliases from a specific directory. Used for testing.
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
                // Navigate to process[0].current.sources
                if let Some(sources) = json
                    .get("process")
                    .and_then(|p| p.get(0))
                    .and_then(|p| p.get("current"))
                    .and_then(|c| c.get("sources"))
                    .and_then(|s| s.as_object())
                {
                    let mut aliases = HashMap::new();
                    for (key, value) in sources {
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
/// If the file does not exist, returns defaults with no warnings.
/// If the file exists but has unknown fields, returns warnings.
pub fn load_config() -> Result<(EzpmConfig, Vec<String>)> {
    let toml_path = Path::new("ezpm.toml");
    if !toml_path.exists() {
        return Ok((EzpmConfig::default(), vec![]));
    }

    let contents = std::fs::read_to_string(toml_path)
        .map_err(|e| anyhow::anyhow!("Failed to read ezpm.toml: {}", e))?;

    let (mut config, warnings) = load_config_from_str(&contents)?;

    // Auto-import aliases from .darklua.json or .luaurc if no aliases in config
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

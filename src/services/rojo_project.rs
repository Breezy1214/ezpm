use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::EzpmConfig;

pub const DEFAULT_PROJECT_TEMPLATE: &str = "default.project.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RojoProjectSettings {
    pub project: PathBuf,
}

impl RojoProjectSettings {
    pub fn from_config(config: &EzpmConfig) -> Self {
        let rojo = config.rojo.as_ref();

        Self {
            project: rojo
                .and_then(|rojo| rojo.project.as_deref())
                .unwrap_or(DEFAULT_PROJECT_TEMPLATE)
                .into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasRojoMapping {
    pub alias_name: String,
    pub alias_path: String,
    pub instance_path: Vec<String>,
}

impl AliasRojoMapping {
    pub fn game_path(&self) -> String {
        self.instance_path.join("/")
    }
}

pub fn game_require(instance_path: &[String]) -> String {
    format!("@game/{}", instance_path.join("/"))
}

fn normalize_alias_path(path: &str) -> String {
    path.trim_end_matches('/').replace('\\', "/")
}

pub fn is_src_alias_path(path: &str, src_prefix: &str) -> bool {
    let normalized = normalize_alias_path(path);
    let normalized_src = normalize_alias_path(src_prefix);
    normalized == normalized_src
        || normalized
            .strip_prefix(&normalized_src)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn default_instance_path(alias_name: &str) -> Vec<String> {
    match alias_name {
        "Client" => vec![
            "StarterPlayer".to_string(),
            "StarterPlayerScripts".to_string(),
            "Client".to_string(),
        ],
        "Server" => vec!["ServerScriptService".to_string(), "Server".to_string()],
        "Shared" => vec!["ReplicatedStorage".to_string(), "Shared".to_string()],
        _ => vec!["ReplicatedStorage".to_string(), alias_name.to_string()],
    }
}

pub fn default_alias_rojo_mappings(
    aliases: &HashMap<String, String>,
    src_prefix: &str,
) -> Vec<AliasRojoMapping> {
    let mut sorted_aliases: Vec<(&String, &String)> = aliases.iter().collect();
    sorted_aliases.sort_by(|(a, _), (b, _)| a.cmp(b));

    sorted_aliases
        .into_iter()
        .filter(|(_, alias_path)| is_src_alias_path(alias_path, src_prefix))
        .map(|(alias_name, alias_path)| AliasRojoMapping {
            alias_name: alias_name.clone(),
            alias_path: normalize_alias_path(alias_path),
            instance_path: default_instance_path(alias_name),
        })
        .collect()
}

fn insert_path_entry(tree: &mut Map<String, Value>, instance_path: &[String], alias_path: &str) {
    let mut current = tree;
    for segment in instance_path
        .iter()
        .take(instance_path.len().saturating_sub(1))
    {
        current = current
            .entry(segment.clone())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("Rojo tree nodes must be objects");
    }

    if let Some(last) = instance_path.last() {
        current.insert(last.clone(), json!({ "$path": alias_path }));
    }
}

pub fn generate_rojo_project(
    project_name: &str,
    aliases: &HashMap<String, String>,
    src_prefix: &str,
) -> String {
    let mut tree: Map<String, Value> = Map::new();
    tree.insert("$className".to_string(), json!("DataModel"));

    for mapping in default_alias_rojo_mappings(aliases, src_prefix) {
        insert_path_entry(&mut tree, &mapping.instance_path, &mapping.alias_path);
    }

    let project_json = json!({
        "name": project_name,
        "tree": Value::Object(tree),
    });

    let mut output = serde_json::to_string_pretty(&project_json).unwrap();
    output.push('\n');
    output
}

fn walk_project_tree(
    node: &Value,
    current_path: &mut Vec<String>,
    aliases_by_path: &HashMap<String, Vec<String>>,
    found_paths: &mut HashMap<String, Vec<String>>,
) {
    let Some(obj) = node.as_object() else {
        return;
    };

    if let Some(path_value) = obj.get("$path").and_then(|value| value.as_str()) {
        let normalized_path = normalize_alias_path(path_value);
        if let Some(alias_names) = aliases_by_path.get(&normalized_path) {
            for alias_name in alias_names {
                found_paths
                    .entry(alias_name.clone())
                    .or_insert_with(|| current_path.clone());
            }
        }
    }

    for (key, child) in obj {
        if key.starts_with('$') {
            continue;
        }
        current_path.push(key.clone());
        walk_project_tree(child, current_path, aliases_by_path, found_paths);
        current_path.pop();
    }
}

pub fn alias_rojo_mappings_from_project_str(
    contents: &str,
    aliases: &HashMap<String, String>,
) -> Result<Vec<AliasRojoMapping>> {
    let json: Value =
        serde_json::from_str(contents).context("default.project.json is invalid JSON")?;
    let tree = json
        .get("tree")
        .and_then(Value::as_object)
        .context("default.project.json is missing a top-level tree object")?;

    let mut aliases_by_path: HashMap<String, Vec<String>> = HashMap::new();
    for (alias_name, alias_path) in aliases {
        aliases_by_path
            .entry(normalize_alias_path(alias_path))
            .or_default()
            .push(alias_name.clone());
    }

    let mut found_paths: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_path = Vec::new();
    walk_project_tree(
        &Value::Object(tree.clone()),
        &mut current_path,
        &aliases_by_path,
        &mut found_paths,
    );

    let mut resolved = Vec::new();
    let mut alias_names: Vec<String> = found_paths.keys().cloned().collect();
    alias_names.sort();

    for alias_name in alias_names {
        let Some(alias_path) = aliases.get(&alias_name) else {
            continue;
        };
        let Some(instance_path) = found_paths.remove(&alias_name) else {
            continue;
        };

        resolved.push(AliasRojoMapping {
            alias_name,
            alias_path: normalize_alias_path(alias_path),
            instance_path,
        });
    }

    Ok(resolved)
}

pub fn alias_rojo_mappings_for_project_root(
    project_root: &Path,
    aliases: &HashMap<String, String>,
    src_prefix: &str,
) -> Vec<AliasRojoMapping> {
    alias_rojo_mappings_for_project(
        project_root,
        Path::new(DEFAULT_PROJECT_TEMPLATE),
        aliases,
        src_prefix,
    )
}

pub fn alias_rojo_mappings_for_project(
    project_root: &Path,
    project_file: &Path,
    aliases: &HashMap<String, String>,
    src_prefix: &str,
) -> Vec<AliasRojoMapping> {
    let mut mappings_by_alias: HashMap<String, AliasRojoMapping> =
        default_alias_rojo_mappings(aliases, src_prefix)
            .into_iter()
            .map(|mapping| (mapping.alias_name.clone(), mapping))
            .collect();

    let project_path = project_root.join(project_file);
    if let Ok(contents) = std::fs::read_to_string(&project_path) {
        if let Ok(project_mappings) = alias_rojo_mappings_from_project_str(&contents, aliases) {
            for mapping in project_mappings {
                mappings_by_alias.insert(mapping.alias_name.clone(), mapping);
            }
        }
    }

    let mut mappings: Vec<AliasRojoMapping> = mappings_by_alias.into_values().collect();
    mappings.sort_by(|a, b| a.alias_name.cmp(&b.alias_name));
    mappings
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_aliases(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_default_alias_rojo_mappings_follow_ezpm_rules() {
        let aliases = make_aliases(&[
            ("Client", "src/client/"),
            ("Libraries", "src/libraries/"),
            ("Packages", "Packages/"),
            ("Server", "src/server/"),
            ("Shared", "src/shared/"),
        ]);

        let mappings = default_alias_rojo_mappings(&aliases, "src");

        let by_alias: HashMap<String, String> = mappings
            .into_iter()
            .map(|mapping| {
                let game_path = mapping.game_path();
                (mapping.alias_name, game_path)
            })
            .collect();

        assert_eq!(
            by_alias.get("Client").map(String::as_str),
            Some("StarterPlayer/StarterPlayerScripts/Client")
        );
        assert_eq!(
            by_alias.get("Server").map(String::as_str),
            Some("ServerScriptService/Server")
        );
        assert_eq!(
            by_alias.get("Shared").map(String::as_str),
            Some("ReplicatedStorage/Shared")
        );
        assert_eq!(
            by_alias.get("Libraries").map(String::as_str),
            Some("ReplicatedStorage/Libraries")
        );
        assert!(
            !by_alias.contains_key("Packages"),
            "external aliases must not appear in Rojo mappings"
        );
    }

    #[test]
    fn test_alias_rojo_mappings_from_project_str_reads_custom_paths() {
        let aliases = make_aliases(&[
            ("Client", "src/player_scripts/"),
            ("Libraries", "src/libraries/"),
        ]);

        let project = r#"{
  "name": "test",
  "tree": {
    "$className": "DataModel",
    "ReplicatedFirst": {
      "Bootstrap": {
        "$path": "src/player_scripts"
      }
    },
    "ReplicatedStorage": {
      "Vendor": {
        "$path": "src/libraries"
      }
    }
  }
}"#;

        let mappings = alias_rojo_mappings_from_project_str(project, &aliases)
            .expect("project mappings should parse");

        let by_alias: HashMap<String, String> = mappings
            .into_iter()
            .map(|mapping| {
                let game_path = mapping.game_path();
                (mapping.alias_name, game_path)
            })
            .collect();

        assert_eq!(
            by_alias.get("Client").map(String::as_str),
            Some("ReplicatedFirst/Bootstrap")
        );
        assert_eq!(
            by_alias.get("Libraries").map(String::as_str),
            Some("ReplicatedStorage/Vendor")
        );
    }

    #[test]
    fn test_alias_rojo_mappings_for_project_root_falls_back_per_missing_alias() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let aliases = make_aliases(&[("Client", "src/client/"), ("Libraries", "src/libraries/")]);

        std::fs::write(
            dir.path().join("default.project.json"),
            r#"{
  "name": "test",
  "tree": {
    "$className": "DataModel",
    "ReplicatedStorage": {
      "Vendor": {
        "$path": "src/libraries"
      }
    }
  }
}"#,
        )
        .expect("failed to write default.project.json");

        let mappings = alias_rojo_mappings_for_project_root(dir.path(), &aliases, "src");
        let by_alias: HashMap<String, String> = mappings
            .into_iter()
            .map(|mapping| {
                let game_path = mapping.game_path();
                (mapping.alias_name, game_path)
            })
            .collect();

        assert_eq!(
            by_alias.get("Libraries").map(String::as_str),
            Some("ReplicatedStorage/Vendor")
        );
        assert_eq!(
            by_alias.get("Client").map(String::as_str),
            Some("StarterPlayer/StarterPlayerScripts/Client")
        );
    }
}

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
    pub alias_root: String,
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
    path.trim_end_matches('/')
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

pub fn is_src_alias_path(path: &str, src_prefix: &str) -> bool {
    let normalized = normalize_alias_path(path);
    let normalized_src = normalize_alias_path(src_prefix);
    normalized == normalized_src
        || normalized
            .strip_prefix(&normalized_src)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn generated_project_instance_path(alias_name: &str) -> Vec<String> {
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
            alias_root: normalize_alias_path(alias_path),
            alias_path: normalize_alias_path(alias_path),
            instance_path: generated_project_instance_path(alias_name),
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
    aliases: &[(String, String)],
    found: &mut Vec<AliasRojoMapping>,
) {
    let Some(obj) = node.as_object() else {
        return;
    };

    if let Some(path_value) = obj.get("$path").and_then(|value| value.as_str()) {
        let project_path = normalize_alias_path(path_value);
        for (alias_name, alias_root) in aliases {
            if project_path == *alias_root {
                found.push(AliasRojoMapping {
                    alias_name: alias_name.clone(),
                    alias_root: alias_root.clone(),
                    alias_path: alias_root.clone(),
                    instance_path: current_path.clone(),
                });
                continue;
            }

            if project_path
                .strip_prefix(&format!("{alias_root}/"))
                .is_some()
            {
                found.push(AliasRojoMapping {
                    alias_name: alias_name.clone(),
                    alias_root: alias_root.clone(),
                    alias_path: project_path.clone(),
                    instance_path: current_path.clone(),
                });
                continue;
            }

            if let Some(relative) = alias_root.strip_prefix(&format!("{project_path}/")) {
                let mut instance_path = current_path.clone();
                instance_path.extend(relative.split('/').map(str::to_string));
                found.push(AliasRojoMapping {
                    alias_name: alias_name.clone(),
                    alias_root: alias_root.clone(),
                    alias_path: alias_root.clone(),
                    instance_path,
                });
            }
        }
    }

    for (key, child) in obj {
        if key.starts_with('$') {
            continue;
        }
        current_path.push(key.clone());
        walk_project_tree(child, current_path, aliases, found);
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

    let normalized_aliases = aliases
        .iter()
        .map(|(name, path)| (name.clone(), normalize_alias_path(path)))
        .collect::<Vec<_>>();
    let mut resolved = Vec::new();
    let mut current_path = Vec::new();
    walk_project_tree(
        &Value::Object(tree.clone()),
        &mut current_path,
        &normalized_aliases,
        &mut resolved,
    );
    resolved.sort_by(|left, right| {
        left.alias_name
            .cmp(&right.alias_name)
            .then_with(|| left.alias_path.cmp(&right.alias_path))
            .then_with(|| left.instance_path.cmp(&right.instance_path))
    });
    resolved.dedup();
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
    _src_prefix: &str,
) -> Vec<AliasRojoMapping> {
    let project_path = project_root.join(project_file);
    std::fs::read_to_string(project_path)
        .ok()
        .and_then(|contents| alias_rojo_mappings_from_project_str(&contents, aliases).ok())
        .unwrap_or_default()
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
    fn test_custom_project_does_not_invent_missing_alias_mappings() {
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
        assert!(!by_alias.contains_key("Client"));
    }

    #[test]
    fn test_project_subdirectory_mapping_keeps_its_real_instance_path() {
        let aliases = make_aliases(&[("Shared", "src/shared/")]);
        let project = r#"{
  "name": "test",
  "tree": {
    "$className": "DataModel",
    "ReplicatedStorage": {
      "Features": {
        "$path": "src/shared/features"
      }
    }
  }
}"#;

        let mappings = alias_rojo_mappings_from_project_str(project, &aliases)
            .expect("project mappings should parse");

        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].alias_root, "src/shared");
        assert_eq!(mappings[0].alias_path, "src/shared/features");
        assert_eq!(mappings[0].game_path(), "ReplicatedStorage/Features");
    }
}

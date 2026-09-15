use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::EzpmConfig;

pub const DEFAULT_PROJECT_TEMPLATE: &str = "default.project.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RojoProjectSettings {
    pub project: PathBuf,
}

impl RojoProjectSettings {
    pub fn from_config(config: &EzpmConfig) -> Self {
        Self {
            project: config
                .rojo
                .as_ref()
                .and_then(|rojo| rojo.project.as_deref())
                .unwrap_or(DEFAULT_PROJECT_TEMPLATE)
                .into(),
        }
    }
}

fn generated_instance_path(alias_name: &str) -> Vec<&str> {
    match alias_name {
        "Client" => vec!["StarterPlayer", "StarterPlayerScripts", "Client"],
        "Server" => vec!["ServerScriptService", "Server"],
        "Shared" => vec!["ReplicatedStorage", "Shared"],
        _ => vec!["ReplicatedStorage", alias_name],
    }
}

fn is_source_alias(path: &str, source_root: &str) -> bool {
    let path = path.trim_end_matches('/').replace('\\', "/");
    let source_root = source_root.trim_end_matches('/').replace('\\', "/");
    path == source_root
        || path
            .strip_prefix(&source_root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn insert_path(tree: &mut Map<String, Value>, instance_path: &[&str], source_path: &str) {
    let mut current = tree;
    for segment in &instance_path[..instance_path.len().saturating_sub(1)] {
        current = current
            .entry((*segment).to_string())
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("generated Rojo nodes are objects");
    }
    if let Some(last) = instance_path.last() {
        current.insert((*last).to_string(), json!({ "$path": source_path }));
    }
}

pub fn generate_rojo_project(
    project_name: &str,
    aliases: &HashMap<String, String>,
    source_root: &str,
) -> String {
    let mut tree = Map::new();
    tree.insert("$className".to_string(), json!("DataModel"));

    let mut aliases = aliases.iter().collect::<Vec<_>>();
    aliases.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (name, path) in aliases {
        if is_source_alias(path, source_root) {
            insert_path(&mut tree, &generated_instance_path(name), path);
        }
    }

    let mut output = serde_json::to_string_pretty(&json!({
        "name": project_name,
        "tree": Value::Object(tree),
    }))
    .expect("generated Rojo project is serializable");
    output.push('\n');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_project_maps_source_aliases_only() {
        let aliases = HashMap::from([
            ("Shared".to_string(), "src/shared/".to_string()),
            ("Packages".to_string(), "Packages/".to_string()),
        ]);
        let project: Value = serde_json::from_str(&generate_rojo_project("Game", &aliases, "src"))
            .expect("valid project");

        assert_eq!(
            project["tree"]["ReplicatedStorage"]["Shared"]["$path"],
            "src/shared/"
        );
        assert!(project["tree"]["ReplicatedStorage"]["Packages"].is_null());
    }
}

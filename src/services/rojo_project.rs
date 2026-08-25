use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use crate::config::{EzpmConfig, RojoPathMapConfig};

pub const DEFAULT_PROJECT_TEMPLATE: &str = "default.project.json";
pub const DEFAULT_GENERATED_PROJECT: &str = "build.project.json";
pub const DEFAULT_SOURCE_ROOT: &str = "src";
pub const DEFAULT_BUILD_ROOT: &str = "darklua_build";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RojoProjectSettings {
    pub project: PathBuf,
    pub generated_project: PathBuf,
    pub path_maps: Vec<RojoPathMapConfig>,
}

impl RojoProjectSettings {
    pub fn from_config(config: &EzpmConfig) -> Self {
        let source = config
            .paths
            .as_ref()
            .and_then(|paths| paths.src.clone())
            .unwrap_or_else(|| DEFAULT_SOURCE_ROOT.to_string());
        let build = config
            .paths
            .as_ref()
            .and_then(|paths| paths.darklua_build.clone())
            .unwrap_or_else(|| DEFAULT_BUILD_ROOT.to_string());
        let rojo = config.rojo.as_ref();
        let path_maps = rojo
            .and_then(|rojo| rojo.path_maps.clone())
            .filter(|maps| !maps.is_empty())
            .unwrap_or_else(|| vec![RojoPathMapConfig { source, build }]);

        Self {
            project: rojo
                .and_then(|rojo| rojo.project.as_deref())
                .unwrap_or(DEFAULT_PROJECT_TEMPLATE)
                .into(),
            generated_project: rojo
                .and_then(|rojo| rojo.generated_project.as_deref())
                .unwrap_or(DEFAULT_GENERATED_PROJECT)
                .into(),
            path_maps,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RojoGenerationResult {
    pub project: PathBuf,
    pub generated_project: PathBuf,
    pub remapped_paths: usize,
    pub written: bool,
}

fn normalize_rojo_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    let has_leading_slash = path.starts_with('/');
    let mut components = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." if components.last().is_some_and(|last| *last != "..") => {
                components.pop();
            }
            other => components.push(other),
        }
    }
    let normalized = components.join("/");
    if has_leading_slash {
        format!("/{normalized}")
    } else {
        normalized
    }
}

fn normalized_path_maps(path_maps: &[RojoPathMapConfig]) -> Result<Vec<(String, String)>> {
    let mut maps = Vec::with_capacity(path_maps.len());
    let mut builds_by_source: HashMap<String, String> = HashMap::new();
    for path_map in path_maps {
        validate_project_relative_path("path map source", Path::new(&path_map.source))?;
        validate_project_relative_path("path map build", Path::new(&path_map.build))?;
        let source = normalize_rojo_path(&path_map.source);
        let build = normalize_rojo_path(&path_map.build);
        if source.is_empty() {
            bail!("Rojo path map source cannot be empty");
        }
        if build.is_empty() {
            bail!("Rojo path map build cannot be empty");
        }
        if let Some(existing_build) = builds_by_source.get(&source) {
            if existing_build != &build {
                bail!(
                    "Rojo path map source '{source}' targets both '{existing_build}' and '{build}'"
                );
            }
            continue;
        }
        builds_by_source.insert(source.clone(), build.clone());
        maps.push((source, build));
    }
    maps.sort_by(|(left, _), (right, _)| {
        right
            .split('/')
            .count()
            .cmp(&left.split('/').count())
            .then_with(|| right.len().cmp(&left.len()))
    });
    Ok(maps)
}

fn validate_project_relative_path(label: &str, path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        bail!("Rojo {label} path cannot be empty");
    }

    let portable = path.to_string_lossy().replace('\\', "/");
    let bytes = portable.as_bytes();
    let has_windows_prefix = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    let has_portable_parent = portable.split('/').any(|component| component == "..");
    let has_native_escape = path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    });

    if portable.starts_with('/') || has_windows_prefix || has_portable_parent || has_native_escape {
        bail!(
            "Rojo {label} path must be project-relative without parent traversal: {}",
            path.display()
        );
    }

    Ok(())
}

fn remap_path(path: &str, path_maps: &[(String, String)]) -> Option<String> {
    let normalized = normalize_rojo_path(path);
    path_maps.iter().find_map(|(source, build)| {
        if normalized == *source {
            return Some(build.clone());
        }
        normalized
            .strip_prefix(source)
            .and_then(|suffix| suffix.strip_prefix('/'))
            .map(|suffix| format!("{build}/{suffix}"))
    })
}

fn remap_project_paths(value: &mut Value, path_maps: &[(String, String)]) -> usize {
    match value {
        Value::Object(object) => {
            let mut remapped = 0;
            if let Some(path) = object.get_mut("$path") {
                if let Some(original) = path.as_str() {
                    if let Some(replacement) = remap_path(original, path_maps) {
                        *path = Value::String(replacement);
                        remapped += 1;
                    }
                }
            }
            remapped
                + object
                    .values_mut()
                    .map(|child| remap_project_paths(child, path_maps))
                    .sum::<usize>()
        }
        Value::Array(array) => array
            .iter_mut()
            .map(|child| remap_project_paths(child, path_maps))
            .sum(),
        _ => 0,
    }
}

pub fn transform_project_template(
    contents: &str,
    path_maps: &[RojoPathMapConfig],
) -> Result<(String, usize)> {
    let mut project: Value =
        serde_json::from_str(contents).context("Rojo project template is invalid JSON")?;
    let object = project
        .as_object()
        .context("Rojo project template must be a top-level JSON object")?;
    object
        .get("tree")
        .and_then(Value::as_object)
        .context("Rojo project template is missing a top-level tree object")?;

    let path_maps = normalized_path_maps(path_maps)?;
    let remapped = remap_project_paths(&mut project, &path_maps);
    let mut output =
        serde_json::to_string_pretty(&project).context("failed to serialize Rojo project")?;
    output.push('\n');
    Ok((output, remapped))
}

fn write_atomic_if_changed(path: &Path, contents: &str) -> Result<bool> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(contents) {
        return Ok(false);
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create a temporary file in {}", parent.display()))?;
    temporary.write_all(contents.as_bytes()).with_context(|| {
        format!(
            "failed to write temporary Rojo project for {}",
            path.display()
        )
    })?;
    temporary.flush().with_context(|| {
        format!(
            "failed to flush temporary Rojo project for {}",
            path.display()
        )
    })?;
    temporary.as_file().sync_all().with_context(|| {
        format!(
            "failed to sync temporary Rojo project for {}",
            path.display()
        )
    })?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("failed to atomically replace {}", path.display()))?;
    Ok(true)
}

pub fn generate_build_project(
    project_root: &Path,
    settings: &RojoProjectSettings,
) -> Result<RojoGenerationResult> {
    validate_project_relative_path("template", &settings.project)?;
    validate_project_relative_path("generated project", &settings.generated_project)?;
    let project = project_root.join(&settings.project);
    let generated_project = project_root.join(&settings.generated_project);
    let same_existing_file = generated_project.exists()
        && std::fs::canonicalize(&project).ok() == std::fs::canonicalize(&generated_project).ok();
    if project == generated_project || same_existing_file {
        bail!(
            "generated Rojo project must differ from its template ({})",
            project.display()
        );
    }
    let contents = std::fs::read_to_string(&project).with_context(|| {
        format!(
            "missing or unreadable Rojo project template {}",
            project.display()
        )
    })?;
    let (output, remapped_paths) = transform_project_template(&contents, &settings.path_maps)
        .with_context(|| format!("failed to transform {}", project.display()))?;
    let written = write_atomic_if_changed(&generated_project, &output)?;

    Ok(RojoGenerationResult {
        project,
        generated_project,
        remapped_paths,
        written,
    })
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
    src_prefix: &str,
) -> Result<Vec<AliasRojoMapping>> {
    let json: Value =
        serde_json::from_str(contents).context("default.project.json is invalid JSON")?;
    let tree = json
        .get("tree")
        .and_then(Value::as_object)
        .context("default.project.json is missing a top-level tree object")?;

    let mut aliases_by_path: HashMap<String, Vec<String>> = HashMap::new();
    for mapping in default_alias_rojo_mappings(aliases, src_prefix) {
        aliases_by_path
            .entry(mapping.alias_path)
            .or_default()
            .push(mapping.alias_name);
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
    let mut mappings_by_alias: HashMap<String, AliasRojoMapping> =
        default_alias_rojo_mappings(aliases, src_prefix)
            .into_iter()
            .map(|mapping| (mapping.alias_name.clone(), mapping))
            .collect();

    let project_path = project_root.join("default.project.json");
    if let Ok(contents) = std::fs::read_to_string(&project_path) {
        if let Ok(project_mappings) =
            alias_rojo_mappings_from_project_str(&contents, aliases, src_prefix)
        {
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

        let mappings = alias_rojo_mappings_from_project_str(project, &aliases, "src")
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

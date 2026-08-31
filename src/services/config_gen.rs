use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

use crate::services::toolchain;

pub fn get_lune_version_from_rokit_contents(contents: &str) -> Option<String> {
    let lune_spec = toolchain::find_tool_spec_in_contents(contents, "lune")?;
    let version = lune_spec.rsplit('@').next()?;

    if version.is_empty() {
        None
    } else {
        Some(version.to_string())
    }
}

pub fn get_lune_version_from_rokit_path(path: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    get_lune_version_from_rokit_contents(&contents)
}

pub fn get_lune_version_in_dir(dir: &Path) -> Option<String> {
    get_lune_version_from_rokit_path(&dir.join("rokit.toml"))
}

pub fn generate_luaurc(aliases: &HashMap<String, String>) -> String {
    generate_luaurc_for_dir(aliases, Path::new("."))
}

pub fn generate_luaurc_for_dir(aliases: &HashMap<String, String>, dir: &Path) -> String {
    generate_luaurc_with_lune_version(aliases, get_lune_version_in_dir(dir).as_deref())
}

fn generate_luaurc_with_lune_version(
    aliases: &HashMap<String, String>,
    lune_version: Option<&str>,
) -> String {
    let mut aliases_obj: serde_json::Map<String, serde_json::Value> =
        serde_json::Map::with_capacity(aliases.len() + 1);

    if let Some(version) = lune_version {
        aliases_obj.insert(
            "lune".to_string(),
            serde_json::Value::String(format!("~/.lune/.typedefs/{}", version)),
        );
    }

    for (k, v) in aliases {
        aliases_obj.insert(k.clone(), serde_json::Value::String(v.clone()));
    }

    let luaurc = json!({
        "aliases": aliases_obj
    });

    let mut output = serde_json::to_string_pretty(&luaurc).unwrap();
    output.push('\n');
    output
}

pub fn write_config_files(dir: &Path, aliases: &HashMap<String, String>) -> Result<()> {
    let luaurc = generate_luaurc_for_dir(aliases, dir);
    std::fs::write(dir.join(".luaurc"), luaurc)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn test_luaurc_includes_lune_alias_when_project_rokit_has_lune() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        let dir = TempDir::new().expect("failed to create temp dir");

        std::fs::write(
            dir.path().join("rokit.toml"),
            "[tools]\nlune = \"lune-org/lune@0.10.4\"\n",
        )
        .expect("write rokit.toml");

        let output = generate_luaurc_for_dir(&aliases, dir.path());
        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("output must be valid JSON");
        let aliases_obj = parsed["aliases"]
            .as_object()
            .expect("aliases must be an object");

        assert_eq!(
            aliases_obj.get("lune").and_then(|value| value.as_str()),
            Some("~/.lune/.typedefs/0.10.4"),
            "lune alias must be emitted when the project opts into lune"
        );
    }

    #[test]
    fn test_luaurc_omits_lune_alias_when_project_rokit_has_no_lune() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        let dir = TempDir::new().expect("failed to create temp dir");

        std::fs::write(
            dir.path().join("rokit.toml"),
            crate::services::toolchain::render_default_rokit_toml(None),
        )
        .expect("write rokit.toml");

        let output = generate_luaurc_for_dir(&aliases, dir.path());
        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("output must be valid JSON");
        let aliases_obj = parsed["aliases"]
            .as_object()
            .expect("aliases must be an object");

        assert!(
            aliases_obj.get("lune").is_none(),
            "lune alias must be omitted when the project rokit.toml does not declare lune"
        );
    }
}

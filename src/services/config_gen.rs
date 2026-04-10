use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

use crate::services::toolchain;

// ─── Public functions ─────────────────────────────────────────────────────────

/// Generate the `.darklua.json` configuration file content.
pub fn generate_darklua_json() -> String {
    static DARKLUA_JSON: OnceLock<String> = OnceLock::new();
    DARKLUA_JSON
        .get_or_init(|| {
            let config = json!({
                "process": [
                    {
                        "rule": "convert_require",
                        "current": { "name": "luau" },
                        "target": {
                            "name": "roblox",
                            "rojo_sourcemap": "sourcemap.json",
                            "indexing_style": "find_first_child"
                        }
                    },
                    "compute_expression",
                    "remove_unused_if_branch",
                    "remove_unused_while",
                    "filter_after_early_return",
                    "remove_nil_declaration",
                    "remove_empty_do"
                ]
            });

            let mut output = serde_json::to_string_pretty(&config).unwrap();
            output.push('\n');
            output
        })
        .clone()
}

/// Read the lune version from supplied `rokit.toml` contents if present.
pub fn get_lune_version_from_rokit_contents(contents: &str) -> Option<String> {
    let lune_spec = toolchain::find_tool_spec_in_contents(contents, "lune")?;
    let version = lune_spec.rsplit('@').next()?;

    if version.is_empty() {
        None
    } else {
        Some(version.to_string())
    }
}

/// Read the lune version from an explicit `rokit.toml` path if present.
pub fn get_lune_version_from_rokit_path(path: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    get_lune_version_from_rokit_contents(&contents)
}

/// Read the lune version from `rokit.toml` inside `dir` if present.
pub fn get_lune_version_in_dir(dir: &Path) -> Option<String> {
    get_lune_version_from_rokit_path(&dir.join("rokit.toml"))
}

/// Read the lune version from `rokit.toml` in the current working directory.
pub fn get_lune_version() -> Option<String> {
    get_lune_version_in_dir(Path::new("."))
}

/// Generate the `.luaurc` configuration file content from an alias map.
pub fn generate_luaurc(aliases: &HashMap<String, String>) -> String {
    generate_luaurc_for_dir(aliases, Path::new("."))
}

/// Generate the `.luaurc` configuration file content using `rokit.toml` from `dir`.
pub fn generate_luaurc_for_dir(aliases: &HashMap<String, String>, dir: &Path) -> String {
    generate_luaurc_with_lune_version(aliases, get_lune_version_in_dir(dir).as_deref())
}

fn generate_luaurc_with_lune_version(
    aliases: &HashMap<String, String>,
    lune_version: Option<&str>,
) -> String {
    let mut aliases_obj: serde_json::Map<String, serde_json::Value> =
        serde_json::Map::with_capacity(aliases.len() + 1);

    // Add lune typedef alias if version is available
    if let Some(version) = lune_version {
        aliases_obj.insert(
            "lune".to_string(),
            serde_json::Value::String(format!("~/.lune/.typedefs/{}", version)),
        );
    }

    // Add user aliases
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

/// generate and write both `.darklua.json` and `.luaurc`
pub fn write_config_files(dir: &Path, aliases: &HashMap<String, String>) -> Result<()> {
    let darklua_json = generate_darklua_json();
    let luaurc = generate_luaurc_for_dir(aliases, dir);

    std::fs::write(dir.join(".darklua.json"), darklua_json)?;
    std::fs::write(dir.join(".luaurc"), luaurc)?;

    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn test_darklua_json_has_convert_require_rule() {
        let output = generate_darklua_json();
        assert!(
            output.contains(r#""rule": "convert_require""#),
            "output must contain convert_require rule: {output}"
        );
        assert!(
            output.contains(r#""name": "luau""#),
            "output must contain current name luau: {output}"
        );
    }

    #[test]
    fn test_darklua_json_has_no_aliases() {
        let output = generate_darklua_json();
        assert!(
            !output.contains("\"aliases\""),
            "output must NOT contain 'aliases' key — DarkLua reads them from .luaurc: {output}"
        );
    }

    #[test]
    fn test_darklua_json_has_optimization_rules() {
        let output = generate_darklua_json();
        let expected_rules = [
            "compute_expression",
            "remove_unused_if_branch",
            "remove_unused_while",
            "filter_after_early_return",
            "remove_nil_declaration",
            "remove_empty_do",
        ];
        for rule in &expected_rules {
            assert!(
                output.contains(rule),
                "output must contain optimization rule '{rule}': {output}"
            );
        }
    }

    #[test]
    fn test_darklua_json_has_rojo_sourcemap() {
        let output = generate_darklua_json();
        assert!(
            output.contains(r#""rojo_sourcemap": "sourcemap.json""#),
            "output must contain rojo_sourcemap: {output}"
        );
    }

    #[test]
    fn test_luaurc_contains_aliases() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("Server".to_string(), "src/server/".to_string());

        let dir = TempDir::new().expect("failed to create temp dir");
        let output = generate_luaurc_for_dir(&aliases, dir.path());

        // Parse the output and check the aliases object
        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("output must be valid JSON");
        let aliases_obj = parsed["aliases"]
            .as_object()
            .expect("aliases must be an object");

        assert_eq!(
            aliases_obj.get("Client").and_then(|v| v.as_str()),
            Some("src/client/"),
            "Client alias must be present with correct path"
        );
        assert_eq!(
            aliases_obj.get("Server").and_then(|v| v.as_str()),
            Some("src/server/"),
            "Server alias must be present with correct path"
        );
    }

    #[test]
    fn test_get_lune_version_from_rokit_contents_reads_explicit_lune() {
        let rokit = r#"
[tools]
lune = "lune-org/lune@0.10.4"
"#;

        assert_eq!(
            get_lune_version_from_rokit_contents(rokit),
            Some("0.10.4".to_string()),
            "lune version should be parsed from supplied rokit.toml contents"
        );
    }

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

    #[test]
    fn test_write_config_files_creates_both() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let aliases = HashMap::new();

        write_config_files(dir.path(), &aliases).expect("write_config_files must succeed");

        assert!(
            dir.path().join(".darklua.json").exists(),
            ".darklua.json must be created"
        );
        assert!(
            dir.path().join(".luaurc").exists(),
            ".luaurc must be created"
        );
    }

    #[test]
    fn test_darklua_json_is_valid_json() {
        let output = generate_darklua_json();
        let result = serde_json::from_str::<serde_json::Value>(&output);
        assert!(
            result.is_ok(),
            "generate_darklua_json output must be valid JSON: {:?}",
            result.err()
        );
    }
}

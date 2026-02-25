use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

// ─── Public functions ─────────────────────────────────────────────────────────

/// Generate the `.darklua.json` configuration file content.
pub fn generate_darklua_json(_aliases: &HashMap<String, String>) -> String {
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
}

/// Read the lune version from `rokit.toml` if present.
pub fn get_lune_version() -> Option<String> {
    let contents = std::fs::read_to_string("rokit.toml").ok()?;
    let parsed: toml::Value = toml::from_str(&contents).ok()?;
    let lune_spec = parsed
        .get("tools")?
        .get("lune")?
        .as_str()?;
    let version = lune_spec.rsplit('@').next()?;
    if version.is_empty() {
        None
    } else {
        Some(version.to_string())
    }
}

/// Generate the `.luaurc` configuration file content from an alias map.
pub fn generate_luaurc(aliases: &HashMap<String, String>) -> String {
    let mut aliases_obj: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    // Add lune typedef alias if version is available
    if let Some(version) = get_lune_version() {
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
    let darklua_json = generate_darklua_json(aliases);
    let luaurc = generate_luaurc(aliases);

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
        let output = generate_darklua_json(&HashMap::new());
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
    fn test_darklua_json_has_no_aliases_when_empty() {
        let output = generate_darklua_json(&HashMap::new());
        assert!(
            !output.contains("\"aliases\""),
            "output must NOT contain 'aliases' key when aliases are empty: {output}"
        );
    }

    #[test]
    fn test_darklua_json_never_includes_aliases() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("Server".to_string(), "src/server/".to_string());

        let output = generate_darklua_json(&aliases);
        assert!(
            !output.contains("\"aliases\""),
            "output must NOT contain 'aliases' key even when aliases are provided: {output}"
        );
    }

    #[test]
    fn test_darklua_json_has_optimization_rules() {
        let output = generate_darklua_json(&HashMap::new());
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
        let output = generate_darklua_json(&HashMap::new());
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

        let output = generate_luaurc(&aliases);

        // Parse the output and check the aliases object
        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("output must be valid JSON");
        let aliases_obj = parsed["aliases"].as_object().expect("aliases must be an object");

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
    fn test_luaurc_lune_alias_depends_on_rokit() {
        let mut aliases = HashMap::new();
        aliases.insert("Client".to_string(), "src/client/".to_string());

        let output = generate_luaurc(&aliases);

        let has_lune_in_rokit = get_lune_version().is_some();
        if has_lune_in_rokit {
            assert!(
                output.contains("\"lune\""),
                "output must contain lune alias when rokit.toml has lune: {output}"
            );
            assert!(
                output.contains(".typedefs/"),
                "lune alias must point to typedefs path: {output}"
            );
        } else {
            assert!(
                !output.contains("\"lune\""),
                "output must NOT contain lune alias when rokit.toml has no lune: {output}"
            );
        }
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
        let output = generate_darklua_json(&HashMap::new());
        let result = serde_json::from_str::<serde_json::Value>(&output);
        assert!(
            result.is_ok(),
            "generate_darklua_json output must be valid JSON: {:?}",
            result.err()
        );
    }
}

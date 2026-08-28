use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

use crate::output;
use crate::services::toolchain;

pub const DEFAULT_DARKLUA_TOML: &str = r#"process = [
    { rule = "convert_require", current = { name = "luau" }, target = { name = "roblox", rojo_sourcemap = "sourcemap.json", indexing_style = "find_first_child" } },
    "make_assignment_local",
    "compute_expression",
    "remove_unused_if_branch",
    "remove_unused_while",
    "filter_after_early_return",
    "remove_nil_declaration",
    "remove_empty_do",
]

[loaders]
"**/*.model.json" = "copy"
"#;

pub fn default_darklua_table() -> toml::value::Table {
    toml::from_str(DEFAULT_DARKLUA_TOML)
        .expect("DEFAULT_DARKLUA_TOML must be a valid darklua table")
}

pub fn generate_darklua_json(
    aliases: &HashMap<String, String>,
    darklua: Option<&toml::value::Table>,
) -> String {
    let default = default_darklua_table();
    let mut table = darklua.unwrap_or(&default).clone();

    if let Some(rules) = table.get_mut("process").and_then(toml::Value::as_array_mut) {
        for rule in rules {
            let Some(rule) = rule.as_table_mut() else {
                continue;
            };
            if rule.get("rule").and_then(toml::Value::as_str) != Some("convert_require") {
                continue;
            }
            let Some(current) = rule.get_mut("current").and_then(toml::Value::as_table_mut) else {
                continue;
            };
            let sources = current
                .entry("sources")
                .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));
            let Some(sources) = sources.as_table_mut() else {
                continue;
            };
            for (name, path) in aliases {
                sources.insert(format!("@{name}"), toml::Value::String(path.clone()));
            }
        }
    }

    let json: serde_json::Value =
        serde_json::to_value(&table).expect("darklua table must convert to JSON");

    let mut output = serde_json::to_string_pretty(&json).unwrap();
    output.push('\n');
    output
}

fn has_convert_require(table: &toml::value::Table) -> bool {
    table
        .get("process")
        .and_then(|process| process.as_array())
        .map(|rules| {
            rules.iter().any(|rule| match rule {
                toml::Value::String(name) => name == "convert_require",
                toml::Value::Table(t) => {
                    t.get("rule").and_then(|v| v.as_str()) == Some("convert_require")
                }
                _ => false,
            })
        })
        .unwrap_or(false)
}

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

pub fn get_lune_version() -> Option<String> {
    get_lune_version_in_dir(Path::new("."))
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

pub fn write_config_files(
    dir: &Path,
    aliases: &HashMap<String, String>,
    darklua: Option<&toml::value::Table>,
) -> Result<()> {
    if let Some(table) = darklua {
        if !has_convert_require(table) {
            output::warn(
                "[darklua] has no `convert_require` rule — `@alias/...` requires will not be rewritten to Roblox paths.",
            );
        }
    }

    let darklua_json = generate_darklua_json(aliases, darklua);
    let luaurc = generate_luaurc_for_dir(aliases, dir);

    std::fs::write(dir.join(".darklua.json"), darklua_json)?;
    std::fs::write(dir.join(".luaurc"), luaurc)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn test_darklua_json_has_alias_sources() {
        let aliases = HashMap::from([
            ("Client".to_string(), "src/client/".to_string()),
            ("Packages".to_string(), "Packages/".to_string()),
        ]);
        let output = generate_darklua_json(&aliases, None);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        let sources = &parsed["process"][0]["current"]["sources"];
        assert!(
            sources["@Client"] == "src/client/" && sources["@Packages"] == "Packages/",
            "DarkLua requires aliases in convert_require.current.sources: {output}"
        );
    }

    #[test]
    fn test_darklua_json_has_make_assignment_local() {
        let output = generate_darklua_json(&HashMap::new(), None);
        let make_local = output.find("make_assignment_local");
        let compute = output.find("compute_expression");

        assert!(
            make_local.is_some(),
            "output must contain the make_assignment_local rule to convert `const` to `local`: {output}"
        );
        assert!(
            matches!((make_local, compute), (Some(m), Some(c)) if m < c),
            "make_assignment_local must run before optimization rules: {output}"
        );
    }

    #[test]
    fn test_darklua_json_copies_rojo_model_files() {
        let output = generate_darklua_json(&HashMap::new(), None);
        let json: serde_json::Value = serde_json::from_str(&output).expect("output is valid JSON");

        assert_eq!(
            json["loaders"]["**/*.model.json"], "copy",
            "Rojo model files must be copied so darklua 0.19 does not convert them to Lua modules: {output}"
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
    fn test_generate_darklua_json_passthrough() {
        let custom: toml::value::Table = toml::from_str(
            r#"process = [
                { rule = "convert_require", current = { name = "luau" }, target = { name = "roblox", rojo_sourcemap = "sourcemap.json" } },
                "rename_variables",
            ]
            "#,
        )
        .expect("custom darklua toml parses");

        let output = generate_darklua_json(&HashMap::new(), Some(&custom));
        let json: serde_json::Value = serde_json::from_str(&output).expect("output is valid JSON");

        let process = json["process"].as_array().expect("process array");
        assert_eq!(process.len(), 2, "custom process length preserved");
        assert_eq!(process[0]["rule"], "convert_require");
        assert_eq!(process[1], "rename_variables", "bare string rule preserved");
        assert!(
            !output.contains("compute_expression"),
            "custom config must replace the default, not merge with it: {output}"
        );
    }

    #[test]
    fn test_has_convert_require_detects_both_forms() {
        let bare: toml::value::Table =
            toml::from_str(r#"process = [ "convert_require", "compute_expression" ]"#)
                .expect("parse bare form");
        assert!(has_convert_require(&bare), "bare-string form detected");

        let table_form: toml::value::Table =
            toml::from_str(r#"process = [ { rule = "convert_require" } ]"#)
                .expect("parse table form");
        assert!(has_convert_require(&table_form), "table form detected");

        let without: toml::value::Table =
            toml::from_str(r#"process = [ "compute_expression" ]"#).expect("parse");
        assert!(!has_convert_require(&without), "absence detected");
    }
}

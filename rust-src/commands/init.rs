use anyhow::{Context, Result};
use inquire::{Confirm, MultiSelect, Select, Text};
use std::collections::HashMap;
use std::path::Path;

use crate::config;
use crate::output;
use crate::services::config_gen;

// ─── Constants ────────────────────────────────────────────────────────────────

const DEFAULT_TOOLS: &[(&str, &str)] = &[
    ("darklua", "seaofvoices/darklua@0.17.3"),
    ("lune", "lune-org/lune@0.10.4"),
    ("rojo", "rojo-rbx/rojo@7.6.1"),
    ("wally", "UpliftGames/wally@0.3.2"),
    ("wally-package-types", "JohnnyMorganz/wally-package-types@1.6.2"),
    ("selene", "Kampfkarren/selene@0.30.0"),
    ("stylua", "JohnnyMorganz/StyLua@2.3.1"),
];

// ─── Public entry point ───────────────────────────────────────────────────────

/// Run the full `ezpm init` wizard: detect existing files, prompt for project
/// details, import or create aliases, scaffold directories, and write all
/// config files (ezpm.toml, .darklua.json, .luaurc, rokit.toml,
/// default.project.json).
pub fn run_init() -> Result<()> {
    // ── Step 1: Detect existing files ────────────────────────────────────────
    let has_ezpm = Path::new("ezpm.toml").exists();
    let has_darklua = Path::new(".darklua.json").exists();
    let has_rokit = Path::new("rokit.toml").exists();
    let has_wally = Path::new("wally.toml").exists();
    let has_project_json = Path::new("default.project.json").exists();

    if has_darklua {
        output::info("Detected .darklua.json");
    }
    if has_rokit {
        output::info("Detected rokit.toml");
    }
    if has_wally {
        output::info("Detected wally.toml");
    }
    if has_project_json {
        output::info("Detected default.project.json");
    }

    // ── Step 2: Handle existing ezpm.toml ────────────────────────────────────
    if has_ezpm {
        let overwrite = Confirm::new("ezpm.toml already exists — overwrite?")
            .with_default(false)
            .prompt()?;
        if !overwrite {
            output::info("Aborted.");
            return Ok(());
        }
    }

    // ── Step 3: Detect project name from default.project.json ────────────────
    let detected_name = if has_project_json {
        read_project_name_from_json("default.project.json")
            .unwrap_or_else(|| "my-roblox-game".to_string())
    } else {
        "my-roblox-game".to_string()
    };

    // ── Step 4: Prompt for project name ──────────────────────────────────────
    let project_name = Text::new("Project name:")
        .with_default(&detected_name)
        .prompt()?;

    // ── Step 5: Prompt for source directory ──────────────────────────────────
    let src_dir = Text::new("Source directory:")
        .with_default("src")
        .prompt()?;

    // ── Step 6a: Handle rokit.toml per-file prompt ───────────────────────────
    if has_rokit {
        let choice = Select::new(
            "rokit.toml already exists —",
            vec!["keep", "overwrite"],
        )
        .prompt()?;
        if choice == "overwrite" {
            let rokit_content = generate_rokit_toml();
            std::fs::write("rokit.toml", rokit_content)
                .context("Failed to write rokit.toml")?;
            output::success("Generated rokit.toml");
        }
    } else {
        let create_rokit =
            Confirm::new("No rokit.toml found — create with default tools?")
                .with_default(true)
                .prompt()?;
        if create_rokit {
            let rokit_content = generate_rokit_toml();
            std::fs::write("rokit.toml", rokit_content)
                .context("Failed to write rokit.toml")?;
            output::success("Generated rokit.toml");
        }
    }

    // ── Step 6b: Handle default.project.json per-file prompt ─────────────────
    let regenerate_project_json = if has_project_json {
        let choice = Select::new(
            "default.project.json already exists —",
            vec!["keep", "overwrite"],
        )
        .prompt()?;
        choice == "overwrite"
    } else {
        true
    };

    // ── Step 7: Handle alias import or defaults ───────────────────────────────
    let mut aliases: HashMap<String, String> = HashMap::new();

    if has_darklua {
        let imported = config::import_aliases_from_dir(Path::new("."));
        if !imported.is_empty() {
            let mut alias_names: Vec<String> = imported.keys().cloned().collect();
            alias_names.sort();

            let selected = MultiSelect::new("Select aliases to import:", alias_names.clone())
                .with_all_selected_by_default()
                .prompt()?;

            for name in selected {
                if let Some(path) = imported.get(&name) {
                    aliases.insert(name, path.clone());
                }
            }
        }
    }

    if aliases.is_empty() {
        let use_defaults = Confirm::new(
            "Use default aliases (Client, Server, Shared, Packages, ServerPackages)?",
        )
        .with_default(true)
        .prompt()?;

        if use_defaults {
            aliases.insert("Client".to_string(), format!("{}/client/", src_dir));
            aliases.insert("Server".to_string(), format!("{}/server/", src_dir));
            aliases.insert("Shared".to_string(), format!("{}/shared/", src_dir));
            aliases.insert("Packages".to_string(), "Packages/".to_string());
            aliases.insert("ServerPackages".to_string(), "ServerPackages/".to_string());
        }
    }

    // ── Step 8: Normalize alias paths (ensure trailing slash) ────────────────
    let aliases: HashMap<String, String> = aliases
        .into_iter()
        .map(|(k, v)| {
            let path = if v.ends_with('/') {
                v
            } else {
                format!("{}/", v)
            };
            (k, path)
        })
        .collect();

    // ── Step 9: Create scaffolding directories for src aliases ────────────────
    let src_prefix = format!("{}/", src_dir);
    for (_alias_name, alias_path) in &aliases {
        if alias_path.starts_with(&src_prefix) {
            // Strip trailing slash for directory creation
            let dir_path = alias_path.trim_end_matches('/');
            let path = Path::new(dir_path);
            if !path.exists() {
                std::fs::create_dir_all(path)
                    .with_context(|| format!("Failed to create directory: {}", dir_path))?;
                output::success(&format!("Created directory: {}", dir_path));
            }
        }
    }

    // ── Step 10: Write ezpm.toml ──────────────────────────────────────────────
    config::save_ezpm_toml(
        Path::new("."),
        &project_name,
        &src_dir,
        "darklua_build",
        &aliases,
    )?;
    output::success("Generated ezpm.toml");

    // ── Step 11: Regenerate .darklua.json and .luaurc ────────────────────────
    if !aliases.is_empty() {
        config_gen::write_config_files(Path::new("."), &aliases)?;
        output::success("Generated .darklua.json");
        output::success("Generated .luaurc");
    }

    // ── Step 12: Generate default.project.json ───────────────────────────────
    if regenerate_project_json {
        let project_json_str = generate_rojo_project(&project_name, &aliases, &src_dir);
        std::fs::write("default.project.json", project_json_str)
            .context("Failed to write default.project.json")?;
        output::success("Generated default.project.json");
    }

    // ── Step 13: Completion message ───────────────────────────────────────────
    output::success("Project initialized!");
    output::hint("Run `ezpm serve` to start developing.");

    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Read the `name` field from a Rojo `default.project.json` file.
/// Returns `None` if the file cannot be read or parsed.
fn read_project_name_from_json(path: &str) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&contents).ok()?;
    json.get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Generate the content of `rokit.toml` with the default set of Roblox tools.
///
/// Matches the Luau `toolchain.generateRokitToml()` output format.
pub fn generate_rokit_toml() -> String {
    let mut output = String::new();
    output.push_str("# Generated by ezpm init\n");
    output.push_str("[tools]\n");
    for (name, spec) in DEFAULT_TOOLS {
        output.push_str(&format!("{} = \"{}\"\n", name, spec));
    }
    output
}

/// Generate the content of `default.project.json` from the alias map.
///
/// Mapping rules:
/// - `Client`  -> `StarterPlayer.StarterPlayerScripts.Client`
/// - `Server`  -> `ServerScriptService.Server`
/// - `Shared`  -> `ReplicatedStorage.Shared`
/// - Unknown aliases under `src_prefix` -> `ReplicatedStorage.<AliasName>`
/// - Non-src aliases (e.g. Packages, ServerPackages) are skipped entirely.
///
/// Path values have trailing slashes stripped for the `$path` entry.
pub fn generate_rojo_project(
    project_name: &str,
    aliases: &HashMap<String, String>,
    src_prefix: &str,
) -> String {
    use serde_json::{json, Map, Value};

    let src_prefix_slash = format!("{}/", src_prefix.trim_end_matches('/'));

    // Build the tree as a mutable JSON object
    let mut tree: Map<String, Value> = Map::new();
    tree.insert("$className".to_string(), json!("DataModel"));

    // Sort alias names for deterministic output
    let mut sorted_aliases: Vec<(&String, &String)> = aliases.iter().collect();
    sorted_aliases.sort_by_key(|(k, _)| k.as_str());

    for (alias_name, alias_path) in sorted_aliases {
        if !alias_path.starts_with(&src_prefix_slash) {
            // Not a src alias — skip (e.g. Packages, ServerPackages)
            continue;
        }

        // Strip trailing slash for the $path value
        let path_value = alias_path.trim_end_matches('/');

        let path_entry = json!({ "$path": path_value });

        match alias_name.as_str() {
            "Client" => {
                // StarterPlayer.StarterPlayerScripts.Client
                let starter_player = tree
                    .entry("StarterPlayer")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .unwrap()
                    .entry("StarterPlayerScripts")
                    .or_insert_with(|| json!({}));
                starter_player
                    .as_object_mut()
                    .unwrap()
                    .insert("Client".to_string(), path_entry);
            }
            "Server" => {
                // ServerScriptService.Server
                tree.entry("ServerScriptService")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .unwrap()
                    .insert("Server".to_string(), path_entry);
            }
            "Shared" => {
                // ReplicatedStorage.Shared
                tree.entry("ReplicatedStorage")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .unwrap()
                    .insert("Shared".to_string(), path_entry);
            }
            _ => {
                // Unknown src alias — ReplicatedStorage.<AliasName>
                tree.entry("ReplicatedStorage")
                    .or_insert_with(|| json!({}))
                    .as_object_mut()
                    .unwrap()
                    .insert(alias_name.clone(), path_entry);
            }
        }
    }

    let project_json = json!({
        "name": project_name,
        "tree": Value::Object(tree),
    });

    let mut output = serde_json::to_string_pretty(&project_json).unwrap();
    output.push('\n');
    output
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── rokit.toml generation ─────────────────────────────────────────────────

    #[test]
    fn test_generate_rokit_toml_contains_all_tools() {
        let output = generate_rokit_toml();

        assert!(
            output.contains("[tools]"),
            "output must contain [tools] section: {output}"
        );

        // Must contain the header comment
        assert!(
            output.contains("# Generated by ezpm init"),
            "output must contain header comment: {output}"
        );

        // Must list all 7 tool names
        let tool_names = [
            "darklua",
            "lune",
            "rojo",
            "wally",
            "wally-package-types",
            "selene",
            "stylua",
        ];
        for name in &tool_names {
            assert!(
                output.contains(name),
                "output must contain tool '{name}': {output}"
            );
        }
    }

    // ── Rojo project generation ───────────────────────────────────────────────

    fn make_aliases(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_generate_rojo_project_maps_client_correctly() {
        let aliases = make_aliases(&[
            ("Client", "src/client/"),
            ("Server", "src/server/"),
            ("Shared", "src/shared/"),
        ]);

        let output = generate_rojo_project("test-game", &aliases, "src");
        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("output must be valid JSON");

        assert_eq!(
            parsed["tree"]["StarterPlayer"]["StarterPlayerScripts"]["Client"]["$path"]
                .as_str(),
            Some("src/client"),
            "Client must map to StarterPlayer.StarterPlayerScripts.Client"
        );

        assert_eq!(
            parsed["tree"]["ServerScriptService"]["Server"]["$path"].as_str(),
            Some("src/server"),
            "Server must map to ServerScriptService.Server"
        );

        assert_eq!(
            parsed["tree"]["ReplicatedStorage"]["Shared"]["$path"].as_str(),
            Some("src/shared"),
            "Shared must map to ReplicatedStorage.Shared"
        );
    }

    #[test]
    fn test_generate_rojo_project_unknown_alias_goes_to_replicated() {
        let aliases = make_aliases(&[("CustomModule", "src/custom/")]);

        let output = generate_rojo_project("test-game", &aliases, "src");
        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("output must be valid JSON");

        assert_eq!(
            parsed["tree"]["ReplicatedStorage"]["CustomModule"]["$path"].as_str(),
            Some("src/custom"),
            "Unknown src alias must go under ReplicatedStorage"
        );
    }

    #[test]
    fn test_generate_rojo_project_skips_non_src_aliases() {
        let aliases = make_aliases(&[
            ("Client", "src/client/"),
            ("Packages", "Packages/"),
            ("ServerPackages", "ServerPackages/"),
        ]);

        let output = generate_rojo_project("test-game", &aliases, "src");
        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("output must be valid JSON");

        // Packages and ServerPackages should not appear in the tree
        assert!(
            parsed["tree"].get("Packages").is_none(),
            "Packages must not appear at tree root"
        );
        assert!(
            parsed["tree"].get("ServerPackages").is_none(),
            "ServerPackages must not appear at tree root"
        );

        // Also not inside ReplicatedStorage
        let rep_storage = &parsed["tree"]["ReplicatedStorage"];
        if !rep_storage.is_null() {
            assert!(
                rep_storage.get("Packages").is_none(),
                "Packages must not appear under ReplicatedStorage"
            );
            assert!(
                rep_storage.get("ServerPackages").is_none(),
                "ServerPackages must not appear under ReplicatedStorage"
            );
        }
    }

    #[test]
    fn test_generate_rojo_project_name_in_output() {
        let aliases = make_aliases(&[("Client", "src/client/")]);
        let output = generate_rojo_project("test-game", &aliases, "src");
        let parsed: serde_json::Value =
            serde_json::from_str(&output).expect("output must be valid JSON");

        assert_eq!(
            parsed["name"].as_str(),
            Some("test-game"),
            "JSON must contain project name"
        );
    }
}

use anyhow::{Context, Result};
use inquire::{Confirm, MultiSelect, Select, Text};
use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::Path;

use crate::config;
use crate::output;
use crate::services::config_gen;
pub use crate::services::rojo_project::generate_rojo_project;
use crate::services::toolchain;

// ─── Public entry point ───────────────────────────────────────────────────────

/// Run the full `ezpm init` wizard: detect existing files, prompt for project
/// details, import or create aliases, scaffold directories, and write all
/// config files (ezpm.toml, .darklua.json, .luaurc, rokit.toml,
/// default.project.json).
pub fn run_init() -> Result<()> {
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("ezpm init requires an interactive terminal");
    }

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
        let choice =
            Select::new("rokit.toml already exists —", vec!["keep", "overwrite"]).prompt()?;
        if choice == "overwrite" {
            let rokit_content = generate_rokit_toml();
            std::fs::write("rokit.toml", rokit_content).context("Failed to write rokit.toml")?;
            output::success("Generated rokit.toml");
        }
    } else {
        let create_rokit = Confirm::new("No rokit.toml found — create with default tools?")
            .with_default(true)
            .prompt()?;
        if create_rokit {
            let rokit_content = generate_rokit_toml();
            std::fs::write("rokit.toml", rokit_content).context("Failed to write rokit.toml")?;
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
        let use_defaults =
            Confirm::new("Use default aliases (Client, Server, Shared, Packages, ServerPackages)?")
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

    // ── Step 9: Create scaffolding directories
    if !Path::new(&src_dir).exists() {
        std::fs::create_dir_all(&src_dir)
            .with_context(|| format!("Failed to create directory: {}", src_dir))?;
        output::success(&format!("Created directory: {}", src_dir));
    }

    for alias_path in aliases.values() {
        let dir_path = alias_path.trim_end_matches('/');
        let path = Path::new(dir_path);
        if !path.exists() {
            std::fs::create_dir_all(path)
                .with_context(|| format!("Failed to create directory: {}", dir_path))?;
            output::success(&format!("Created directory: {}", dir_path));
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
        config_gen::write_config_files(Path::new("."), &aliases, None)?;
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
    toolchain::render_default_rokit_toml(Some("# Generated by ezpm init"))
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::toolchain;
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
        assert_eq!(
            output,
            toolchain::render_default_rokit_toml(Some("# Generated by ezpm init")),
            "generated rokit.toml must exactly match the shared embedded manifest"
        );
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
            parsed["tree"]["StarterPlayer"]["StarterPlayerScripts"]["Client"]["$path"].as_str(),
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

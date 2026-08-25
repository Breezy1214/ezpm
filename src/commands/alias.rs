use anyhow::Result;
use inquire::{Confirm, MultiSelect, Select, Text};
use std::collections::HashMap;
use std::path::Path;

use crate::config;
use crate::output;
use crate::services::config_gen;

pub fn alias_menu() -> Result<()> {
    let options = vec![
        "Add Alias",
        "Remove Alias",
        "List Aliases",
        "Sync from ezpm.toml",
        "Back",
    ];

    loop {
        let selection = match Select::new("Alias Management:", options.clone()).prompt() {
            Ok(s) => s,
            Err(_) => break,
        };

        match selection {
            "Add Alias" => {
                if let Err(e) = alias_add() {
                    output::error(&format!("{}", e));
                }
            }
            "Remove Alias" => {
                if let Err(e) = alias_remove() {
                    output::error(&format!("{}", e));
                }
            }
            "List Aliases" => {
                let aliases = config::load_config().ok().and_then(|(c, _)| c.aliases);
                if let Err(e) = alias_list(&aliases) {
                    output::error(&format!("{}", e));
                }
            }
            "Sync from ezpm.toml" => {
                if let Err(e) = alias_sync() {
                    output::error(&format!("{}", e));
                }
            }
            _ => break,
        }

        output::print_line("");
    }

    Ok(())
}

pub fn alias_add() -> Result<()> {
    let name = Text::new("Alias name (e.g., Client):").prompt()?;
    if name.trim().is_empty() {
        output::info("Aborted.");
        return Ok(());
    }

    let raw_path = Text::new("Path (e.g., src/client/):").prompt()?;
    if raw_path.trim().is_empty() {
        output::info("Aborted.");
        return Ok(());
    }

    let path = if raw_path.ends_with('/') {
        raw_path
    } else {
        format!("{}/", raw_path)
    };

    let (cfg, _warnings) = config::load_config()?;

    let mut aliases: HashMap<String, String> = cfg.aliases.unwrap_or_default();

    aliases.insert(name.clone(), path.clone());

    let project_name = cfg
        .project
        .as_ref()
        .and_then(|p| p.name.as_deref())
        .unwrap_or("project")
        .to_string();
    let src_dir = cfg
        .paths
        .as_ref()
        .and_then(|p| p.src.as_deref())
        .unwrap_or("src")
        .to_string();
    let darklua_build = cfg
        .paths
        .as_ref()
        .and_then(|p| p.darklua_build.as_deref())
        .unwrap_or("darklua_build")
        .to_string();

    config::save_aliases_preserving_config(
        Path::new("."),
        &project_name,
        &src_dir,
        &darklua_build,
        &aliases,
    )?;

    config_gen::write_config_files(Path::new("."), &aliases, cfg.darklua.as_ref())?;

    let path_no_slash = path.trim_end_matches('/');
    if !Path::new(path_no_slash).exists() {
        let create = Confirm::new(&format!("Create directory '{}'?", path_no_slash))
            .with_default(true)
            .prompt()?;
        if create {
            std::fs::create_dir_all(path_no_slash)?;
        }
    }

    output::success(&format!("Added alias @{} -> {}", name, path));
    Ok(())
}

pub fn alias_remove() -> Result<()> {
    let (cfg, _warnings) = config::load_config()?;

    let mut aliases: HashMap<String, String> = cfg.aliases.unwrap_or_default();

    if aliases.is_empty() {
        output::info("No aliases configured.");
        return Ok(());
    }

    let mut sorted_names: Vec<String> = aliases.keys().cloned().collect();
    sorted_names.sort();

    let labels: Vec<String> = sorted_names
        .iter()
        .map(|name| format!("{} -> {}", name, aliases[name]))
        .collect();

    let selected = MultiSelect::new("Select aliases to remove:", labels).prompt()?;

    if selected.is_empty() {
        output::info("No aliases selected.");
        return Ok(());
    }

    let confirm = Confirm::new(&format!("Remove {} alias(es)?", selected.len()))
        .with_default(false)
        .prompt()?;

    if !confirm {
        return Ok(());
    }

    let names_to_remove: Vec<String> = selected
        .iter()
        .map(|label| label.split(" -> ").next().unwrap_or("").to_string())
        .collect();

    for name in &names_to_remove {
        aliases.remove(name);
    }

    let project_name = cfg
        .project
        .as_ref()
        .and_then(|p| p.name.as_deref())
        .unwrap_or("project")
        .to_string();
    let src_dir = cfg
        .paths
        .as_ref()
        .and_then(|p| p.src.as_deref())
        .unwrap_or("src")
        .to_string();
    let darklua_build = cfg
        .paths
        .as_ref()
        .and_then(|p| p.darklua_build.as_deref())
        .unwrap_or("darklua_build")
        .to_string();

    config::save_aliases_preserving_config(
        Path::new("."),
        &project_name,
        &src_dir,
        &darklua_build,
        &aliases,
    )?;

    config_gen::write_config_files(Path::new("."), &aliases, cfg.darklua.as_ref())?;

    output::success(&format!("Removed {} alias(es).", names_to_remove.len()));
    for name in &names_to_remove {
        output::print_line(&format!("  - @{}", name));
    }

    Ok(())
}

pub fn alias_list(aliases: &Option<HashMap<String, String>>) -> Result<()> {
    let aliases = match aliases {
        Some(m) if !m.is_empty() => m,
        _ => {
            output::info("No aliases configured.");
            return Ok(());
        }
    };

    let mut sorted_names: Vec<&String> = aliases.keys().collect();
    sorted_names.sort();

    let max_len = sorted_names.iter().map(|n| n.len()).max().unwrap_or(0);

    for name in &sorted_names {
        output::print_line(&format!(
            "@{:<width$} -> {}",
            name,
            aliases[*name],
            width = max_len
        ));
    }

    output::print_line(&format!("\n{} alias(es) configured.", aliases.len()));
    Ok(())
}

pub fn alias_sync() -> Result<()> {
    let (cfg, _warnings) = config::load_config()?;

    let aliases = match cfg.aliases {
        Some(ref m) if !m.is_empty() => m,
        _ => {
            output::info("No aliases found in ezpm.toml.");
            return Ok(());
        }
    };

    config_gen::write_config_files(Path::new("."), aliases, cfg.darklua.as_ref())?;

    output::success(&format!("Synced {} aliases from ezpm.toml", aliases.len()));
    output::info("Regenerated .darklua.json and .luaurc");
    Ok(())
}

use anyhow::Result;
use inquire::{Confirm, MultiSelect, Select, Text};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

use crate::config::{self, EzpmConfig};
use crate::diagnostics;
use crate::error::EzpmError;
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

        let result = match selection {
            "Add Alias" => alias_add(None, None, false),
            "Remove Alias" => alias_remove(&[]),
            "List Aliases" => {
                let aliases = config::load_config().ok().and_then(|(c, _)| c.aliases);
                alias_list(&aliases)
            }
            "Sync from ezpm.toml" => alias_sync(),
            _ => break,
        };
        if let Err(e) = result {
            output::error(&format!("{}", e));
        }

        output::print_line("");
    }

    Ok(())
}

/// `Shared`, `@Shared` → `Shared`. Names become `@Name/...` requires, so no spaces or slashes.
pub fn normalize_alias_name(raw: &str) -> Result<String, EzpmError> {
    let name = raw.trim().trim_start_matches('@');
    if name.is_empty() {
        return Err(EzpmError::usage("alias name is empty"));
    }
    if name.contains(|c: char| c.is_whitespace() || c == '/' || c == '\\') {
        return Err(EzpmError::usage(format!(
            "alias name '{name}' must not contain whitespace or slashes"
        )));
    }
    Ok(name.to_string())
}

/// `src\shared`, `./src/shared` → `src/shared/`.
pub fn normalize_alias_path(raw: &str) -> Result<String, EzpmError> {
    let path = config::normalize_relative(raw);
    if path.is_empty() {
        return Err(EzpmError::usage("alias path is empty"));
    }
    Ok(format!("{path}/"))
}

fn save_aliases(cfg: &EzpmConfig, aliases: &HashMap<String, String>) -> Result<()> {
    let project_name = cfg
        .project
        .as_ref()
        .and_then(|p| p.name.as_deref())
        .unwrap_or("project")
        .to_string();
    config::save_aliases_preserving_config(Path::new("."), &project_name, &cfg.src_dir(), aliases)?;
    config_gen::write_config_files(Path::new("."), aliases)
}

pub fn alias_add(name: Option<String>, path: Option<String>, create_dir: bool) -> Result<()> {
    let interactive = name.is_none() || path.is_none();
    if interactive && !output::can_prompt() {
        return Err(EzpmError::usage("alias add needs a NAME and a PATH")
            .with_hint("ezpm alias add Shared src/shared/")
            .into());
    }

    let name = match name {
        Some(name) => name,
        None => {
            let name = Text::new("Alias name (e.g., Client):").prompt()?;
            if name.trim().is_empty() {
                output::info("Aborted.");
                return Ok(());
            }
            name
        }
    };
    let name = normalize_alias_name(&name)?;

    let path = match path {
        Some(path) => path,
        None => {
            let path = Text::new("Path (e.g., src/client/):").prompt()?;
            if path.trim().is_empty() {
                output::info("Aborted.");
                return Ok(());
            }
            path
        }
    };
    let path = normalize_alias_path(&path)?;

    let (mut cfg, _warnings) = config::load_config()?;
    let mut aliases: HashMap<String, String> = cfg.aliases.take().unwrap_or_default();
    let replaced = aliases.insert(name.clone(), path.clone());
    save_aliases(&cfg, &aliases)?;

    let dir = path.trim_end_matches('/');
    let mut created_dir = false;
    if !Path::new(dir).exists() {
        let create = create_dir
            || (interactive
                && Confirm::new(&format!("Create directory '{dir}'?"))
                    .with_default(true)
                    .prompt()?);
        if create {
            std::fs::create_dir_all(dir)?;
            created_dir = true;
        } else if !interactive {
            output::warn(&format!(
                "'{dir}' does not exist yet; pass --create-dir to create it"
            ));
        }
    }

    if output::is_json() {
        output::document(&json!({
            "changed": replaced.as_deref() != Some(path.as_str()),
            "alias": { "name": name, "path": path },
            "replaced": replaced,
            "created_dir": created_dir,
        }));
    } else {
        match replaced {
            Some(old) if old != path => {
                output::success(&format!("Changed alias @{name}: {old} -> {path}"))
            }
            _ => output::success(&format!("Added alias @{name} -> {path}")),
        }
    }
    Ok(())
}

pub fn alias_remove(names: &[String]) -> Result<()> {
    let (mut cfg, _warnings) = config::load_config()?;
    let mut aliases: HashMap<String, String> = cfg.aliases.take().unwrap_or_default();

    let names_to_remove = if names.is_empty() {
        if !output::can_prompt() {
            return Err(EzpmError::usage("alias remove needs at least one NAME")
                .with_hint("ezpm alias remove Shared")
                .into());
        }
        match prompt_names_to_remove(&aliases)? {
            Some(names) => names,
            None => return Ok(()),
        }
    } else {
        let mut resolved = Vec::new();
        for raw in names {
            let name = normalize_alias_name(raw)?;
            if !aliases.contains_key(&name) {
                let known: Vec<&str> = aliases.keys().map(String::as_str).collect();
                let mut error = EzpmError::project(format!("no alias named '{name}'"));
                if let Some(suggestion) = diagnostics::closest(&name, &known) {
                    error = error.with_hint(format!("did you mean '{suggestion}'?"));
                }
                return Err(error.into());
            }
            if !resolved.contains(&name) {
                resolved.push(name);
            }
        }
        resolved
    };

    for name in &names_to_remove {
        aliases.remove(name);
    }
    save_aliases(&cfg, &aliases)?;

    if output::is_json() {
        output::document(&json!({ "changed": true, "removed": names_to_remove }));
    } else {
        output::success(&format!("Removed {} alias(es).", names_to_remove.len()));
        for name in &names_to_remove {
            output::print_line(&format!("  - @{}", name));
        }
    }

    Ok(())
}

fn prompt_names_to_remove(aliases: &HashMap<String, String>) -> Result<Option<Vec<String>>> {
    if aliases.is_empty() {
        output::info("No aliases configured.");
        return Ok(None);
    }

    let mut sorted_names: Vec<String> = aliases.keys().cloned().collect();
    sorted_names.sort();
    let labels: Vec<String> = sorted_names
        .iter()
        .map(|name| format!("{} -> {}", name, aliases[name]))
        .collect();

    let selected = MultiSelect::new("Select aliases to remove:", labels).raw_prompt()?;
    if selected.is_empty() {
        output::info("No aliases selected.");
        return Ok(None);
    }

    let confirm = Confirm::new(&format!("Remove {} alias(es)?", selected.len()))
        .with_default(false)
        .prompt()?;
    if !confirm {
        return Ok(None);
    }

    Ok(Some(
        selected
            .iter()
            .map(|option| sorted_names[option.index].clone())
            .collect(),
    ))
}

pub fn alias_list(aliases: &Option<HashMap<String, String>>) -> Result<()> {
    if output::is_json() {
        let sorted: std::collections::BTreeMap<_, _> = aliases.iter().flatten().collect();
        output::document(&json!({ "aliases": sorted }));
        return Ok(());
    }

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
    let aliases = cfg.aliases.unwrap_or_default();

    if aliases.is_empty() {
        if output::is_json() {
            output::document(&json!({ "changed": false, "synced": 0 }));
        } else {
            output::info("No aliases found in ezpm.toml.");
        }
        return Ok(());
    }

    config_gen::write_config_files(Path::new("."), &aliases)?;

    if output::is_json() {
        output::document(&json!({ "changed": true, "synced": aliases.len() }));
    } else {
        output::success(&format!("Synced {} aliases from ezpm.toml", aliases.len()));
        output::info("Regenerated .luaurc");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_names_and_paths_are_normalized() {
        assert_eq!(normalize_alias_name(" @Shared ").unwrap(), "Shared");
        assert!(normalize_alias_name("a b").is_err());
        assert!(normalize_alias_name("a/b").is_err());
        assert_eq!(
            normalize_alias_path("./src\\shared").unwrap(),
            "src/shared/"
        );
        assert!(normalize_alias_path(" ").is_err());
    }
}

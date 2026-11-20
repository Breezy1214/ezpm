use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;

use crate::{
    config::EzpmConfig,
    diagnostics::{self, Diagnostic},
    error::EzpmError,
    output,
    services::{path_filter, rojo_project::RojoProjectSettings, sourcemap},
};

/// Prints where each file lands in the game and the require that reaches it.
pub fn run(config: &EzpmConfig, paths: &[String]) -> Result<()> {
    let project_dir = std::env::current_dir().context("could not determine current directory")?;
    let rojo = RojoProjectSettings::from_config(config);
    let index = sourcemap::generate_index(&project_dir, &rojo.project)?;

    let mut entries = Vec::new();
    let mut found = Vec::new();
    for raw in paths {
        let absolute = path_filter::absolute(&project_dir, Path::new(raw));
        let path = path_filter::display(&project_dir, &absolute);
        match index.placement(&absolute) {
            Some(placement) => {
                let require = index
                    .game_path(&absolute)
                    .map(|game_path| format!("require(\"{game_path}\")"));
                entries.push(json!({
                    "path": path,
                    "instance_path": placement.instance_path,
                    "class": placement.class_name,
                    "require": require,
                }));
            }
            None => {
                let message = if absolute.exists() {
                    "not in the Rojo project, so it is not synced into the game"
                } else {
                    "no such file"
                };
                found.push(Diagnostic::new("where.unmapped", message).at(path.clone(), None));
                entries.push(json!({ "path": path, "instance_path": Value::Null }));
            }
        }
    }

    if output::is_json() {
        output::document(&json!({ "entries": entries, "diagnostics": found }));
    } else {
        for entry in &entries {
            if entry["instance_path"].is_null() {
                continue;
            }
            output::print_line(&format!(
                "{} \u{2192} {} ({})",
                entry["path"].as_str().unwrap_or_default(),
                entry["instance_path"].as_str().unwrap_or_default(),
                entry["class"].as_str().unwrap_or_default(),
            ));
            if let Some(require) = entry["require"].as_str() {
                output::print_line(&format!("  {require}"));
            }
        }
        diagnostics::report(&found);
    }

    if !found.is_empty() {
        return Err(EzpmError::reported("", 1).into());
    }
    Ok(())
}

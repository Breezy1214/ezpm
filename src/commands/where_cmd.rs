use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::json;
use std::path::Path;

use crate::{
    config::EzpmConfig,
    diagnostics::{self, Diagnostic},
    error::EzpmError,
    output,
    services::{path_filter, rojo_project::RojoProjectSettings, sourcemap},
};

/// Where one path lands; the instance fields are `null` for a path Rojo doesn't sync.
#[derive(Serialize)]
struct Entry {
    path: String,
    instance_path: Option<String>,
    class: Option<String>,
    require: Option<String>,
}

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
        let placement = index.placement(&absolute);
        if placement.is_none() {
            let message = if absolute.exists() {
                "not in the Rojo project, so it is not synced into the game"
            } else {
                "no such file"
            };
            found.push(Diagnostic::new("where.unmapped", message).at(path.clone(), None));
        }
        entries.push(Entry {
            path,
            instance_path: placement.map(|p| p.instance_path.clone()),
            class: placement.map(|p| p.class_name.clone()),
            require: index
                .game_path(&absolute)
                .map(|game_path| format!("require(\"{game_path}\")")),
        });
    }

    if output::is_json() {
        output::document(&json!({ "entries": entries, "diagnostics": found }));
    } else {
        for entry in &entries {
            let (Some(instance_path), Some(class)) = (&entry.instance_path, &entry.class) else {
                continue;
            };
            output::print_line(&format!(
                "{} \u{2192} {instance_path} ({class})",
                entry.path
            ));
            if let Some(require) = &entry.require {
                output::print_line(&format!("  {require}"));
            }
        }
        diagnostics::report(&found);
    }

    if !found.is_empty() {
        return Err(EzpmError::exit(1).into());
    }
    Ok(())
}

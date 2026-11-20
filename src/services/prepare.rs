use anyhow::{Context, Result};
use std::path::Path;

use crate::config::EzpmConfig;
use crate::services::{config_gen, rojo_project::RojoProjectSettings};

/// What every command that hands the project to Rojo does first: write `.luaurc` from
/// `ezpm.toml` and resolve which Rojo project file to use.
pub fn prepare_project(project_dir: &Path, config: &EzpmConfig) -> Result<RojoProjectSettings> {
    let aliases = config.aliases.clone().unwrap_or_default();
    config_gen::write_config_files(project_dir, &aliases)
        .context("failed to generate .luaurc from ezpm.toml")?;
    Ok(RojoProjectSettings::from_config(config))
}

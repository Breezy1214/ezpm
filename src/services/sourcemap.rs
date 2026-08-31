use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::services::toolchain;

#[derive(Debug)]
pub struct SourcemapResult {
    pub success: bool,
    pub stderr: String,
}

pub fn generate_sourcemap_for_project(
    project_dir: &Path,
    project: &Path,
) -> Result<SourcemapResult> {
    let output = Command::new("rojo")
        .arg("sourcemap")
        .arg(project)
        .arg("-o")
        .arg("sourcemap.json")
        .current_dir(project_dir)
        .output()
        .with_context(|| toolchain::missing_tool_context("rojo"))?;

    Ok(SourcemapResult {
        success: output.status.success(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

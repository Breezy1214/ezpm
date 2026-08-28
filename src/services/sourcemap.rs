use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::services::darklua_runner::DarkluaResult;
use crate::services::toolchain;

pub fn generate_sourcemap(project_dir: &Path) -> Result<DarkluaResult> {
    generate_sourcemap_for_project(project_dir, Path::new("build.project.json"))
}

pub fn generate_sourcemap_for_project(project_dir: &Path, project: &Path) -> Result<DarkluaResult> {
    let output = Command::new("rojo")
        .arg("sourcemap")
        .arg(project)
        .arg("-o")
        .arg("sourcemap.json")
        .current_dir(project_dir)
        .output()
        .with_context(|| toolchain::missing_tool_context("rojo"))?;

    Ok(DarkluaResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

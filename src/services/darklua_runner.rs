use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::services::toolchain;

#[derive(Debug)]
pub struct DarkluaResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub fn process_tree(src: &Path, build: &Path) -> Result<DarkluaResult> {
    let output = Command::new("darklua")
        .arg("process")
        .arg(src)
        .arg(build)
        .output()
        .with_context(|| toolchain::missing_tool_context("darklua"))?;

    Ok(DarkluaResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

pub fn process_file(src_file: &Path, build_file: &Path) -> Result<DarkluaResult> {
    let output = Command::new("darklua")
        .arg("process")
        .arg(src_file)
        .arg(build_file)
        .output()
        .with_context(|| toolchain::missing_tool_context("darklua"))?;

    Ok(DarkluaResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

pub fn process_tree_with_retry(src: &Path, build: &Path) -> Result<DarkluaResult> {
    let result = process_tree(src, build)?;
    if result.success && !result.stderr.trim().is_empty() {
        let retry = process_tree(src, build)?;
        return Ok(retry);
    }
    Ok(result)
}

pub fn clean_build_dir(build: &Path) -> Result<()> {
    if build.exists() {
        std::fs::remove_dir_all(build)
            .with_context(|| format!("Failed to remove build directory: {}", build.display()))?;
    }
    std::fs::create_dir_all(build)
        .with_context(|| format!("Failed to create build directory: {}", build.display()))?;
    Ok(())
}

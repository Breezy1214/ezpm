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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_clean_build_dir_creates_fresh() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let build = dir.path().join("build");

        std::fs::create_dir_all(&build).expect("create build dir");
        std::fs::write(build.join("artifact.lua"), b"-- old artifact").expect("write artifact");

        clean_build_dir(&build).expect("clean_build_dir must succeed");

        assert!(build.exists(), "build dir must still exist after clean");
        let entries: Vec<_> = std::fs::read_dir(&build).expect("read build dir").collect();
        assert!(
            entries.is_empty(),
            "build dir must be empty after clean, found {} entries",
            entries.len()
        );
    }

    #[test]
    fn test_clean_build_dir_creates_if_missing() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let build = dir.path().join("nonexistent_build");

        assert!(!build.exists(), "build dir must not exist before clean");

        clean_build_dir(&build).expect("clean_build_dir must succeed on nonexistent dir");

        assert!(
            build.exists(),
            "build dir must be created by clean_build_dir"
        );
        assert!(build.is_dir(), "build path must be a directory");
    }
}

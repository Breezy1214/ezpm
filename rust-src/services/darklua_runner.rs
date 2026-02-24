use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

// ─── Public types ─────────────────────────────────────────────────────────────

/// Result of running a darklua or rojo subprocess.
#[derive(Debug)]
pub struct DarkluaResult {
    /// Whether the process exited successfully (exit code 0).
    pub success: bool,
    /// Standard output captured from the process.
    pub stdout: String,
    /// Standard error captured from the process.
    pub stderr: String,
    /// Exit code of the process (0 on success).
    pub exit_code: i32,
}

// ─── Public functions ─────────────────────────────────────────────────────────

/// Run `darklua process <src> <build>` to transform an entire source tree.
///
/// This is the full-tree variant used by the `build` command in Phase 3.
/// Uses synchronous `std::process::Command` (Phase 2 is intentionally
/// synchronous — async subprocess orchestration is Phase 4).
pub fn process_tree(src: &Path, build: &Path) -> Result<DarkluaResult> {
    let output = Command::new("darklua")
        .arg("process")
        .arg(src)
        .arg(build)
        .output()
        .context("Failed to run darklua. Is it installed? (rokit install)")?;

    Ok(DarkluaResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Run `darklua process <src_file> <build_file>` on a single file.
///
/// Used for incremental builds in the serve watch pipeline (Phase 4).
pub fn process_file(src_file: &Path, build_file: &Path) -> Result<DarkluaResult> {
    let output = Command::new("darklua")
        .arg("process")
        .arg(src_file)
        .arg(build_file)
        .output()
        .context("Failed to run darklua. Is it installed? (rokit install)")?;

    Ok(DarkluaResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Clean and recreate the build directory.
///
/// Removes the directory recursively if it exists, then creates a fresh empty
/// directory. Per CONTEXT.md decision: "Always clean the build/ directory
/// before full builds".
pub fn clean_build_dir(build: &Path) -> Result<()> {
    if build.exists() {
        std::fs::remove_dir_all(build)
            .with_context(|| format!("Failed to remove build directory: {}", build.display()))?;
    }
    std::fs::create_dir_all(build)
        .with_context(|| format!("Failed to create build directory: {}", build.display()))?;
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_clean_build_dir_creates_fresh() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let build = dir.path().join("build");

        // Create build dir with a file inside
        std::fs::create_dir_all(&build).expect("create build dir");
        std::fs::write(build.join("artifact.lua"), b"-- old artifact")
            .expect("write artifact");

        // Clean it
        clean_build_dir(&build).expect("clean_build_dir must succeed");

        // Dir must exist but be empty
        assert!(build.exists(), "build dir must still exist after clean");
        let entries: Vec<_> = std::fs::read_dir(&build)
            .expect("read build dir")
            .collect();
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

        // Build dir does not exist
        assert!(!build.exists(), "build dir must not exist before clean");

        clean_build_dir(&build).expect("clean_build_dir must succeed on nonexistent dir");

        assert!(build.exists(), "build dir must be created by clean_build_dir");
        assert!(build.is_dir(), "build path must be a directory");
    }
}

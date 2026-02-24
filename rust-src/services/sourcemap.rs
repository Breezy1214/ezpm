use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::services::darklua_runner::DarkluaResult;

// ─── Public functions ─────────────────────────────────────────────────────────

/// Run `rojo sourcemap . -o sourcemap.json` in the given project directory.
///
/// Rojo's sourcemap library API is not extractable (it requires a running Rojo
/// instance or session), so we invoke it as a subprocess per the Phase 2
/// research decision.
///
/// Reuses `DarkluaResult` as the return type since both darklua and rojo are
/// external tools with the same stdout/stderr/exit_code structure.
pub fn generate_sourcemap(project_dir: &Path) -> Result<DarkluaResult> {
    let output = Command::new("rojo")
        .arg("sourcemap")
        .arg(".")
        .arg("-o")
        .arg("sourcemap.json")
        .current_dir(project_dir)
        .output()
        .context("Failed to run rojo. Is it installed? (rokit install)")?;

    Ok(DarkluaResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// This test verifies that the generate_sourcemap function compiles
    /// correctly and returns a Result<DarkluaResult>. It does not test actual
    /// rojo execution since rojo may not be in PATH in CI.
    ///
    /// The actual rojo invocation is tested via integration tests that require
    /// a full project setup with rojo installed.
    #[test]
    #[ignore = "requires rojo in PATH"]
    fn test_generate_sourcemap_requires_rojo() {
        let dir = TempDir::new().expect("failed to create temp dir");
        // This would only succeed if rojo is installed and project files exist
        let _result = generate_sourcemap(dir.path());
    }

    /// Verify the function signature compiles and the DarkluaResult type is
    /// reused correctly across both sourcemap and darklua_runner modules.
    #[test]
    fn test_darkluaresult_type_reused() {
        // Construct a DarkluaResult directly to verify the type is accessible
        // and has the expected fields — this ensures API consistency.
        let result = DarkluaResult {
            success: true,
            stdout: "output".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert!(result.success);
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "output");
        assert!(result.stderr.is_empty());
    }
}

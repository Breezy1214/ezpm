use anyhow::{Context, Result};
use std::process::Command;

use crate::output;

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Check whether a tool binary is available in PATH by invoking `--version`
/// silently with `.output()` (Pitfall 4 from RESEARCH.md — captured, not
/// passed-through).
fn is_tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ─── Public functions ─────────────────────────────────────────────────────────

/// Run Selene and StyLua --check on the source directory (QUAL-01, QUAL-02).
///
/// Skips gracefully if either or both tools are not installed. Reports lint
/// issues but never returns an `Err` — matching the Luau `runLinting` behaviour
/// where issues are printed but execution continues (non-fatal lint run).
pub fn lint(src_path: &str) -> Result<()> {
    let has_selene = is_tool_available("selene");
    let has_stylua = is_tool_available("stylua");

    // ── Announce availability ────────────────────────────────────────────────
    if !has_selene {
        output::info("Skipping Selene (not installed)");
    }
    if !has_stylua {
        output::info("Skipping StyLua (not installed)");
    }

    // ── Graceful skip if no tools at all (QUAL-02) ───────────────────────────
    if !has_selene && !has_stylua {
        output::info("No linting tools installed.");
        return Ok(());
    }

    let mut issues_found = false;

    // ── Selene ───────────────────────────────────────────────────────────────
    if has_selene {
        if output::is_verbose() {
            let selene_status = Command::new("selene")
                .arg(src_path)
                .status()
                .context("Failed to run selene")?;

            if !selene_status.success() {
                output::warn("Selene linting found issues");
                issues_found = true;
            }
        } else {
            let selene_out = Command::new("selene")
                .arg(src_path)
                .output()
                .context("Failed to run selene")?;

            if !selene_out.status.success() {
                output::warn("Selene linting found issues");
                issues_found = true;
            }
        }
    }

    // ── StyLua --check ───────────────────────────────────────────────────────
    if has_stylua {
        if output::is_verbose() {
            let stylua_status = Command::new("stylua")
                .arg("--check")
                .arg(src_path)
                .status()
                .context("Failed to run stylua")?;

            if !stylua_status.success() {
                output::warn("StyLua formatting issues found. Run `ezpm format` to fix.");
                issues_found = true;
            }
        } else {
            let stylua_out = Command::new("stylua")
                .arg("--check")
                .arg(src_path)
                .output()
                .context("Failed to run stylua")?;

            if !stylua_out.status.success() {
                output::warn("StyLua formatting issues found. Run `ezpm format` to fix.");
                issues_found = true;
            }
        }
    }

    // ── Summary ───────────────────────────────────────────────────────────────
    if !issues_found {
        output::success("All code quality checks passed!");
    }

    Ok(())
}

/// Run StyLua on the source directory to apply formatting in-place (QUAL-03).
///
/// Skips gracefully if StyLua is not installed, printing an installation hint.
pub fn format_code(src_path: &str) -> Result<()> {
    if !is_tool_available("stylua") {
        output::info("StyLua is not installed.");
        output::hint("Install with: rokit add JohnnyMorganz/StyLua@2.3.1");
        return Ok(());
    }

    if output::is_verbose() {
        let stylua_status = Command::new("stylua")
            .arg(src_path)
            .status()
            .context("Failed to run stylua")?;

        if !stylua_status.success() {
            anyhow::bail!(
                "stylua formatting failed with exit code: {:?}",
                stylua_status.code()
            );
        }
    } else {
        let stylua_out = Command::new("stylua")
            .arg(src_path)
            .output()
            .context("Failed to run stylua")?;

        if !stylua_out.status.success() {
            anyhow::bail!(
                "stylua formatting failed with exit code: {:?}",
                stylua_out.status.code()
            );
        }
    }

    output::success("Code formatted successfully!");
    Ok(())
}

/// Launch the Moonwave documentation server (QUAL-04).
///
/// Gated on `docs_enabled` from the `[display]` config section. When enabled,
/// this is a **blocking** call — Moonwave runs as a long-lived server and the
/// user exits with Ctrl-C.
pub fn docs(docs_enabled: bool) -> Result<()> {
    if !docs_enabled {
        output::info("Documentation is not set up for this project.");
        output::hint("Set docs_enabled = true in ezpm.toml [display] section.");
        return Ok(());
    }

    // Blocking pass-through — moonwave dev is a long-running server.
    Command::new("moonwave")
        .arg("dev")
        .status()
        .context("Failed to run moonwave. Is it installed? (rokit install)")?;

    Ok(())
}

use anyhow::{Context, Result};
use std::process::Command;

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
        println!("Skipping Selene (not installed)");
    }
    if !has_stylua {
        println!("Skipping StyLua (not installed)");
    }

    // ── Graceful skip if no tools at all (QUAL-02) ───────────────────────────
    if !has_selene && !has_stylua {
        println!("No linting tools installed.");
        return Ok(());
    }

    let mut issues_found = false;

    // ── Selene ───────────────────────────────────────────────────────────────
    if has_selene {
        let selene_status = Command::new("selene")
            .arg(src_path)
            .status()
            .context("Failed to run selene")?;

        if !selene_status.success() {
            println!("Selene linting found issues");
            issues_found = true;
        }
    }

    // ── StyLua --check ───────────────────────────────────────────────────────
    if has_stylua {
        let stylua_status = Command::new("stylua")
            .arg("--check")
            .arg(src_path)
            .status()
            .context("Failed to run stylua")?;

        if !stylua_status.success() {
            println!("StyLua formatting issues found. Run `ezpm format` to fix.");
            issues_found = true;
        }
    }

    // ── Summary ───────────────────────────────────────────────────────────────
    if !issues_found {
        println!("All code quality checks passed!");
    }

    Ok(())
}

/// Run StyLua on the source directory to apply formatting in-place (QUAL-03).
///
/// Skips gracefully if StyLua is not installed, printing an installation hint.
pub fn format_code(src_path: &str) -> Result<()> {
    if !is_tool_available("stylua") {
        println!(
            "StyLua is not installed. Install with: rokit add JohnnyMorganz/StyLua@2.3.1"
        );
        return Ok(());
    }

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

    println!("Code formatted successfully!");
    Ok(())
}

/// Launch the Moonwave documentation server (QUAL-04).
///
/// Gated on `docs_enabled` from the `[display]` config section. When enabled,
/// this is a **blocking** call — Moonwave runs as a long-lived server and the
/// user exits with Ctrl-C.
pub fn docs(docs_enabled: bool) -> Result<()> {
    if !docs_enabled {
        println!(
            "Documentation is not set up for this project. \
             Set `docs_enabled = true` in ezpm.toml [display] section."
        );
        return Ok(());
    }

    // Blocking pass-through — moonwave dev is a long-running server.
    Command::new("moonwave")
        .arg("dev")
        .status()
        .context("Failed to run moonwave. Is it installed? (rokit install)")?;

    Ok(())
}

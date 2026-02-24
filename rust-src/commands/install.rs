use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::services::sourcemap;

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

/// Run `rokit install`, then optionally `wally install` + `wally-package-types`
/// if a `wally.toml` exists in the current directory (INST-01, INST-02,
/// INST-03).
///
/// Uses `.status()` for all subprocess calls so output streams to the user's
/// terminal in real-time (Pitfall 4 — don't use `.output()` for long-running
/// tools).
pub fn install_tools(_src_prefix: &str) -> Result<()> {
    println!("Installing development tools...");

    // ── 1. Rokit install ────────────────────────────────────────────────────
    let rokit_status = Command::new("rokit")
        .arg("install")
        .status()
        .context("Failed to run rokit. Is it installed?")?;

    if !rokit_status.success() {
        anyhow::bail!(
            "rokit install failed with exit code: {:?}",
            rokit_status.code()
        );
    }

    // ── 2. Wally (optional) ─────────────────────────────────────────────────
    if Path::new("wally.toml").exists() {
        println!("Installing Wally packages...");

        let wally_status = Command::new("wally")
            .arg("install")
            .status()
            .context("Failed to run wally. Is it installed? (rokit install)")?;

        if wally_status.success() {
            // Run wally-package-types for Packages/
            if Path::new("Packages").exists() {
                Command::new("wally-package-types")
                    .arg("--sourcemap")
                    .arg("sourcemap.json")
                    .arg("Packages")
                    .status()
                    .context("Failed to run wally-package-types")?;
            }

            // Run wally-package-types for ServerPackages/ if it was created
            // (INST-03)
            if Path::new("ServerPackages").exists() {
                Command::new("wally-package-types")
                    .arg("--sourcemap")
                    .arg("sourcemap.json")
                    .arg("ServerPackages")
                    .status()
                    .context("Failed to run wally-package-types for ServerPackages")?;
            }
        }
    }

    println!("All tools installed successfully!");
    Ok(())
}

/// Clean and re-install Wally packages from scratch (INST-04).
///
/// Sequence: remove lock + sourcemap + package dirs → `wally install` →
/// `rojo sourcemap` → `wally-package-types` for each package dir →
/// `rojo sourcemap` again (matches Luau `setupWallyPackages` behavior).
pub fn setup_wally_packages(_src_prefix: &str) -> Result<()> {
    // ── Gate: wally.toml must exist ─────────────────────────────────────────
    if !Path::new("wally.toml").exists() {
        println!("No wally.toml found, skipping.");
        return Ok(());
    }

    println!("Setting up Wally packages...");
    println!("Clearing current systems...");

    // ── Remove stale artefacts ───────────────────────────────────────────────
    if Path::new("sourcemap.json").exists() {
        std::fs::remove_file("sourcemap.json")
            .context("Failed to remove sourcemap.json")?;
    }

    if Path::new("wally.lock").exists() {
        std::fs::remove_file("wally.lock")
            .context("Failed to remove wally.lock")?;
    }

    for pkg_dir in &["Packages", "ServerPackages"] {
        if Path::new(pkg_dir).exists() {
            std::fs::remove_dir_all(pkg_dir)
                .with_context(|| format!("Failed to remove {pkg_dir}/"))?;
        }
    }

    // ── Wally install ────────────────────────────────────────────────────────
    println!("Installing Wally packages...");

    let wally_status = Command::new("wally")
        .arg("install")
        .status()
        .context("Failed to run wally. Is it installed? (rokit install)")?;

    if !wally_status.success() {
        anyhow::bail!(
            "wally install failed with exit code: {:?}",
            wally_status.code()
        );
    }

    // ── First sourcemap pass ─────────────────────────────────────────────────
    println!("Generating source map...");

    let cwd =
        std::env::current_dir().context("Failed to determine current directory")?;
    let sm_result = sourcemap::generate_sourcemap(&cwd)
        .context("Failed to generate sourcemap")?;

    if !sm_result.success {
        eprintln!("Warning: sourcemap generation failed: {}", sm_result.stderr);
    }

    // ── wally-package-types for each package directory ───────────────────────
    for pkg_dir in &["Packages", "ServerPackages"] {
        if Path::new(pkg_dir).exists() {
            println!("Setting up types for {pkg_dir}...");

            Command::new("wally-package-types")
                .arg("--sourcemap")
                .arg("sourcemap.json")
                .arg(pkg_dir)
                .status()
                .context("Failed to run wally-package-types")?;
        }
    }

    // ── Second sourcemap pass (matches Luau behaviour) ───────────────────────
    let sm_result2 = sourcemap::generate_sourcemap(&cwd)
        .context("Failed to generate final sourcemap")?;

    if !sm_result2.success {
        eprintln!(
            "Warning: final sourcemap generation failed: {}",
            sm_result2.stderr
        );
    }

    println!("Setup complete!");
    Ok(())
}

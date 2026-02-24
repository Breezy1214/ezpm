use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::output;
use crate::services::sourcemap;

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Check whether a tool binary is available in PATH by invoking `--version`
/// silently with `.output()` (Pitfall 4 from RESEARCH.md — captured, not
/// passed-through).
#[allow(dead_code)]
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
/// In default mode subprocess output is captured so the spinner can show
/// cleanly. In --verbose mode subprocess output streams to the terminal.
pub fn install_tools(_src_prefix: &str) -> Result<()> {
    let pb = output::start_spinner("Installing development tools...");

    // ── 1. Rokit install ────────────────────────────────────────────────────
    if output::is_verbose() {
        pb.suspend(|| {});
        let rokit_status = Command::new("rokit")
            .arg("install")
            .status()
            .context("Failed to run rokit. Is it installed?")?;

        if !rokit_status.success() {
            pb.finish_and_clear();
            anyhow::bail!(
                "rokit install failed with exit code: {:?}",
                rokit_status.code()
            );
        }
    } else {
        let rokit_out = Command::new("rokit")
            .arg("install")
            .output()
            .context("Failed to run rokit. Is it installed?")?;

        if !rokit_out.status.success() {
            pb.finish_and_clear();
            anyhow::bail!(
                "rokit install failed with exit code: {:?}",
                rokit_out.status.code()
            );
        }
    }

    // ── 2. Wally (optional) ─────────────────────────────────────────────────
    if Path::new("wally.toml").exists() {
        pb.set_message("Installing Wally packages...");

        if output::is_verbose() {
            pb.suspend(|| {});
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
        } else {
            let wally_out = Command::new("wally")
                .arg("install")
                .output()
                .context("Failed to run wally. Is it installed? (rokit install)")?;

            if wally_out.status.success() {
                // Run wally-package-types for Packages/
                if Path::new("Packages").exists() {
                    Command::new("wally-package-types")
                        .arg("--sourcemap")
                        .arg("sourcemap.json")
                        .arg("Packages")
                        .output()
                        .context("Failed to run wally-package-types")?;
                }

                // Run wally-package-types for ServerPackages/ if it was created
                // (INST-03)
                if Path::new("ServerPackages").exists() {
                    Command::new("wally-package-types")
                        .arg("--sourcemap")
                        .arg("sourcemap.json")
                        .arg("ServerPackages")
                        .output()
                        .context("Failed to run wally-package-types for ServerPackages")?;
                }
            }
        }
    }

    pb.finish_and_clear();
    output::success("All tools installed successfully!");
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
        output::info("No wally.toml found, skipping.");
        return Ok(());
    }

    let pb = output::start_spinner("Setting up Wally packages...");
    pb.set_message("Clearing current systems...");

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
    pb.set_message("Installing Wally packages...");

    if output::is_verbose() {
        pb.suspend(|| {});
        let wally_status = Command::new("wally")
            .arg("install")
            .status()
            .context("Failed to run wally. Is it installed? (rokit install)")?;

        if !wally_status.success() {
            pb.finish_and_clear();
            anyhow::bail!(
                "wally install failed with exit code: {:?}",
                wally_status.code()
            );
        }
    } else {
        let wally_out = Command::new("wally")
            .arg("install")
            .output()
            .context("Failed to run wally. Is it installed? (rokit install)")?;

        if !wally_out.status.success() {
            pb.finish_and_clear();
            anyhow::bail!(
                "wally install failed with exit code: {:?}",
                wally_out.status.code()
            );
        }
    }

    // ── First sourcemap pass ─────────────────────────────────────────────────
    pb.set_message("Generating source map...");

    let cwd =
        std::env::current_dir().context("Failed to determine current directory")?;
    let sm_result = sourcemap::generate_sourcemap(&cwd)
        .context("Failed to generate sourcemap")?;

    if !sm_result.success {
        pb.suspend(|| output::warn(&format!("Warning: sourcemap generation failed: {}", sm_result.stderr)));
    }

    // ── wally-package-types for each package directory ───────────────────────
    for pkg_dir in &["Packages", "ServerPackages"] {
        if Path::new(pkg_dir).exists() {
            pb.set_message(format!("Setting up types for {pkg_dir}..."));

            if output::is_verbose() {
                pb.suspend(|| {});
                Command::new("wally-package-types")
                    .arg("--sourcemap")
                    .arg("sourcemap.json")
                    .arg(pkg_dir)
                    .status()
                    .context("Failed to run wally-package-types")?;
            } else {
                Command::new("wally-package-types")
                    .arg("--sourcemap")
                    .arg("sourcemap.json")
                    .arg(pkg_dir)
                    .output()
                    .context("Failed to run wally-package-types")?;
            }
        }
    }

    // ── Second sourcemap pass (matches Luau behaviour) ───────────────────────
    pb.set_message("Finalizing...");
    let sm_result2 = sourcemap::generate_sourcemap(&cwd)
        .context("Failed to generate final sourcemap")?;

    if !sm_result2.success {
        pb.suspend(|| output::warn(&format!(
            "Warning: final sourcemap generation failed: {}",
            sm_result2.stderr
        )));
    }

    pb.finish_and_clear();
    output::success("Wally packages set up!");
    Ok(())
}
